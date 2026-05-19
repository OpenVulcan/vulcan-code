//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。
mod attachment_management;
mod composer_attachments;
mod configuration;
mod context_menu;
mod helpers;
mod media_preview;
mod oauth_flow;
mod pending_prompt_queue;
mod session_loading;
mod session_projection;
mod subagent_runtime;
mod task_tree;
mod views;

use self::helpers::*;
use self::oauth_flow::*;

use crate::agent_profile::{self, AgentProfileCatalog, SYSTEM_DEFAULT_AGENT_CHOICE_ID};
use crate::agent_runtime::config::{
    EXPLORE_AGENT_ID, PLAN_AGENT_ID, WORKSPACE_COORDINATOR_AGENT_ID,
};
use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::task_runtime::DetachedTaskRuntimeEvent;
use crate::agent_runtime::{
    self, ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
    ExecutionStreamEvent, RuntimeFoundation, RuntimeSkillKind, stream_runtime_run,
};
use crate::agent_view::{
    agent_view_block_collapsed_by_default, agent_view_block_id, agent_view_document_raw_source,
    build_agent_view_document,
};
use crate::ai_tool_configuration::{self, AiToolConfigurationLoadSource};
use crate::attachment_asset_index::{
    AttachmentCleanupPreview, AttachmentCleanupReport, AttachmentCleanupSelection,
    execute_attachment_cleanup, preview_attachment_cleanup,
};
use crate::code_view;
use crate::composer_parts::{
    build_composer_input_parts, composer_parts_have_submittable_content, composer_parts_label_text,
    composer_parts_preview_text, composer_parts_to_user_payload, composer_parts_transcript_body,
};
use crate::context_budget::{
    ContextBudgetSnapshot, ContextCompactionPolicy, build_projected_session_context_budget_snapshot,
};
use crate::conversation_protocol::{
    SessionConversationRecord, SessionConversationState, append_assistant_text_message,
    append_record, append_record_body_delta, append_user_text_message,
    compact_conversation_records_after_summary, compact_conversation_with_summary,
    conversation_state_to_context_records, conversation_state_to_session_messages,
};
use crate::data_plane::{self, ControllerBootstrapReport, ControllerRuntimeHandle};
use crate::diff::{DiffFile, WorkspaceDiffPanel};
use crate::luaskills_runtime::{
    self, LuaSkillsBootstrapReport, LuaSkillsCatalog, LuaSkillsCatalogSkill,
    LuaSkillsInstallSourceChoice, LuaSkillsManagementAction, LuaSkillsManagementReport,
    LuaSkillsRuntimeHandle,
};
use crate::mcp::{McpRuntimeAccess, bootstrap_mcp_runtime_access};
use crate::model::{
    ActivityEntry, ActivityLevel, ExecutionMode, PaneKind, PendingPromptModelReference,
    PendingPromptPanelEntry, PendingPromptPanelEntryState, PendingPromptRuntimeOptions,
    PendingPromptWebSearchOverride, PendingSessionPromptEntry, PendingSessionPromptSource,
    SessionCompactionEntry, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, TaskProjectSummary,
};
use crate::project_temp_maintenance;
use crate::provider_capabilities::resolve_effective_model_capabilities;
use crate::provider_config::{
    self, AiToolKind, ModelChoice, NativeBridgeRoutingMode, PersistedAiToolConfiguration,
    PersistedControllerConfiguration, PersistedMcpOAuthConfig, PersistedMcpServerEntry,
    PersistedMcpTransportKind, PersistedModelOverrideEntry, PersistedProviderEntry,
    PersistedProviderOptions, ProviderChoice, ProviderConfigurationFile, ProviderFieldKey,
    ProviderRuntimeKind, ReasoningEffortLevel, ReasoningSelectionOption, WebSearchRoutingMode,
};
use crate::selectable_transcript::{
    SelectableTranscriptCommand, SelectableTranscriptContextMenuMediaKind,
    SelectableTranscriptContextMenuRequest, SelectableTranscriptDragAutoScroll,
    SelectableTranscriptDragAutoScrollDirection, SelectableTranscriptInteraction,
    SelectableTranscriptSurfaceId, view_selectable_rendered_transcript_owned_with_context_menu,
    view_selectable_rendered_transcript_shared_with_context_menu,
    view_selectable_rendered_transcript_with_context_menu,
};
use crate::skills::{
    CompactionSkillInput, ExploreSkillInput, SessionDerivedRecord, SessionDerivedSkillChain,
    SummarySkillInput, TitleAgentInput,
};
use crate::style;
use crate::theme::{self, ThemePaletteExt};
use crate::wheel_passthrough::{WheelIntercept, WheelPassthrough};
use crate::workspace_path::normalize_workspace_path_text;
use crate::workspace_store::{
    self, SESSION_HISTORY_SEARCH_RESULT_LIMIT, SESSION_TOOL_DETAIL_PAGE_LIMIT,
    SESSION_TRANSCRIPT_WINDOW_LIMIT, SessionHistoryPage, SessionHydrationSnapshot,
    SessionMaintenanceReport, SessionToolDetailPage, WorkspaceHistorySearchPage, WorkspaceSnapshot,
};
use iced::widget::operation;
use iced::widget::pane_grid::{self, Configuration, PaneGrid};
use iced::widget::{
    PickList, button, checkbox, container, image as iced_image, markdown, mouse_area,
    pick_list as iced_pick_list, progress_bar, radio, row, rule, scrollable, slider, space, stack,
    svg, text, text_editor, text_input, toggler, tooltip,
};
use iced::window;
use iced::{
    Center, ContentFit, Element, Fill, FillPortion, Pixels, Shrink, Subscription, Task, Theme,
    clipboard, keyboard, task,
};
use iced_aw::date_picker::Date;
use iced_fonts::{CODICON_FONT_BYTES, LUCIDE_FONT_BYTES, codicon, lucide};
use rodio::{Decoder, OutputStream, Sink};
use serde_json::{Value, json};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use vulcan_agent_view::{
    THINKING_MODAL_ANIMATION_DURATION, THINKING_MODAL_TICK_INTERVAL, ThinkingModalPhase,
    build_modal_thinking_surface_document, build_modal_tool_surface_document,
    build_overlay_thinking_surface_document, build_thinking_title, format_thinking_duration_label,
    parse_agent_view_document, resolve_video_thumbnail_preview, thinking_modal_progress,
    thinking_overlay_scrollable_id, view_thinking_modal_surface_with_context_menu,
    view_thinking_overlay_surface_with_context_menu,
};
use vulcan_core::types::CoreEventKind as VulcanCoreEventKind;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

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

/// Maximum number of queued follow-up prompts retained per session before new manual submissions are rejected.
/// 每个会话在拒绝新的手动提交之前最多保留的排队跟进提示词数量。
const SESSION_PENDING_PROMPT_QUEUE_LIMIT: usize = 10;

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
/// Conservative estimated row height used when scrolling to one searched transcript hit.
/// 滚动定位某条搜索命中时使用的保守估算行高。
const WORKSPACE_HISTORY_SEARCH_SCROLL_ESTIMATED_ROW_HEIGHT: f32 = 132.0;

/// Maximum session messages retained in GUI memory for one hydrated transcript window.
/// 单个已水合转录窗口允许保留在 GUI 内存中的最大会话消息数量。
const SESSION_IN_MEMORY_MESSAGE_LIMIT: usize = 4000;

/// Shared square size used by the floating transcript jump-to-bottom button.
/// 转录区浮动回底按钮使用的共享方形尺寸。
const TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE: f32 = 38.0;

/// Shared bottom inset used by the floating transcript jump-to-bottom button overlay.
/// 转录区浮动回底按钮覆盖层使用的共享底部内边距。
const TRANSCRIPT_JUMP_TO_BOTTOM_OVERLAY_BOTTOM_INSET: u16 = 26;

/// Fixed height used by the D-shaped subagent handle and the minimum active-task list panel.
/// D 形子代理把手以及活动任务列表面板最小高度使用的固定高度。
const SUBAGENT_NAVIGATION_PANEL_HEIGHT: f32 = 100.0;

/// Vertical padding reserved by the active subagent dispatch list panel.
/// 活动子代理调度列表面板预留的垂直内边距。
const SUBAGENT_NAVIGATION_PANEL_VERTICAL_PADDING: f32 = 16.0;

/// Top offset used by the left-edge subagent dispatch navigation overlay inside transcript surfaces.
/// 子代理调度导航覆盖层在转录表面内使用的顶部偏移。
const SUBAGENT_NAVIGATION_OVERLAY_TOP_OFFSET: f32 = 32.0;

/// Left offset used to keep the subagent dispatch navigation list attached to the transcript edge.
/// 用于让子代理调度导航列表贴住转录边缘的左侧偏移。
const SUBAGENT_NAVIGATION_OVERLAY_LEFT_OFFSET: f32 = -1.0;

/// Collapsed width used by the D-shaped subagent dispatch navigation handle.
/// D 形子代理调度导航把手使用的折叠宽度。
const SUBAGENT_NAVIGATION_COLLAPSED_WIDTH: f32 = 24.0;

/// Total width used when the subagent dispatch navigation hover detail window is open.
/// 子代理调度导航悬停详情窗口打开时使用的总宽度。
const SUBAGENT_NAVIGATION_EXPANDED_WIDTH: f32 = 244.0;

/// Fixed height used by one subagent dispatch navigation row inside the expanded panel.
/// 展开的子代理调度导航面板内单条任务行使用的固定高度。
const SUBAGENT_NAVIGATION_ROW_HEIGHT: f32 = 30.0;

/// Vertical spacing used between subagent dispatch navigation rows.
/// 子代理调度导航任务行之间使用的垂直间距。
const SUBAGENT_NAVIGATION_ROW_GAP: f32 = 2.0;

/// Shared animation tick used by drag-driven transcript auto-scroll while the pointer is kept outside the visible viewport.
/// 当指针持续停留在可见视口之外时，拖拽驱动的转录自动滚动使用的共享动画 Tick。
const TRANSCRIPT_DRAG_AUTO_SCROLL_TICK_INTERVAL: Duration = Duration::from_millis(16);

/// Shared coalescing tick used to keep streaming transcript tail-follow work bounded to frame cadence.
/// 用于将流式转录尾部追随工作合并到帧节奏内的共享 Tick。
const TRANSCRIPT_STREAM_FOLLOW_TAIL_TICK_INTERVAL: Duration = Duration::from_millis(50);

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

/// Shared glyph-width ratio used to estimate the intrinsic width of one monospace diff editor line.
/// 用于估算等宽 Diff 编辑器单行固有宽度的共享字宽比例。
const DIFF_CODE_GLYPH_WIDTH_RATIO: f32 = 0.56;

/// Shared minimal intrinsic width reserved for one diff hunk editor when the current longest line is very short.
/// 当当前最长行很短时为单个 Diff hunk 编辑器预留的共享最小固有宽度。
const DIFF_CODE_EDITOR_MIN_CONTENT_WIDTH: f32 = 160.0;

/// Shared maximal intrinsic width reserved for one diff hunk editor to avoid pathological layout explosions.
/// 为避免病态布局膨胀而给单个 Diff hunk 编辑器设置的共享最大固有宽度。
const DIFF_CODE_EDITOR_MAX_CONTENT_WIDTH: f32 = 12_000.0;

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

/// Stable widget identifier used by the inline workbench model-picker filter input.
/// 工作台内嵌模型选择弹窗过滤输入框使用的稳定部件标识。
const WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID: &str =
    "vulcan-code.workbench-model-picker-filter-input";
/// Stable widget identifier used by the left-panel workspace-history search input.
/// 左侧面板工作台历史搜索输入框使用的稳定部件标识。
const WORKSPACE_HISTORY_SEARCH_INPUT_ID: &str = "vulcan-code.workspace-history-search-input";
/// Stable widget identifier used by the left-panel search project-filter input.
/// 左侧搜索项目过滤输入框使用的稳定部件标识。
const WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID: &str =
    "vulcan-code.workspace-history-search-project-filter-input";

/// Shared logical line height used by the main prompt composer.
/// 主提示词编辑器使用的共享逻辑行高。
const COMPOSER_EDITOR_LINE_HEIGHT: f32 = 24.0;

/// Shared text size used by the main prompt composer editor content.
/// 主提示词编辑器正文内容使用的共享字号。
const COMPOSER_EDITOR_TEXT_SIZE: f32 = 16.0;

/// Shared combined vertical inset used by the main prompt composer surface.
/// 主提示词编辑器表面使用的共享纵向合计内边距。
const COMPOSER_EDITOR_VERTICAL_INSET: f32 = 20.0;

/// Shared combined horizontal inset used by the main prompt composer surface.
/// 主提示词编辑器表面使用的共享横向合计内边距。
const COMPOSER_EDITOR_HORIZONTAL_INSET: f32 = 24.0;

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

/// Shared fixed row height reserved for the attachment strip shown under the main prompt composer editor.
/// 主提示词编辑器下方附件条保留的共享固定行高。
const COMPOSER_ATTACHMENT_ROW_HEIGHT: f32 = 74.0;

/// Shared square edge length used by each staged composer attachment preview tile.
/// 每个已暂存提示词附件预览方格使用的共享边长。
const COMPOSER_ATTACHMENT_TILE_SIZE: f32 = 64.0;

/// Shared square edge length used by the circular remove button on an attachment tile.
/// 附件方格右上角圆形移除按钮使用的共享边长。
const COMPOSER_ATTACHMENT_REMOVE_BUTTON_SIZE: f32 = 20.0;

/// Maximum staged attachment count accepted by the main composer.
/// 主提示词输入框允许暂存的最大附件数量。
const MAX_COMPOSER_ATTACHMENTS: usize = 10;

/// Shared fixed horizontal inset kept on both sides of the pending-prompt queue panel above the composer.
/// 主提示词输入框上方待发送队列面板两侧保留的共享水平留白。
const COMPOSER_PENDING_QUEUE_PANEL_SIDE_INSET: f32 = 25.0;

/// Shared fixed row height used by each pending-prompt queue entry line.
/// 每一条待发送队列项使用的共享固定行高。
const COMPOSER_PENDING_QUEUE_ROW_HEIGHT: f32 = 36.0;

/// Shared vertical padding used by the pending-prompt queue panel container.
/// 待发送队列面板容器使用的共享垂直内边距。
const COMPOSER_PENDING_QUEUE_PANEL_VERTICAL_PADDING: f32 = 8.0;

/// Shared gap used between pending-prompt queue rows.
/// 待发送队列行之间使用的共享间距。
const COMPOSER_PENDING_QUEUE_ROW_GAP: f32 = 6.0;

/// Shared indicator height shown while one pending-prompt queue row is being reordered.
/// 当某条待发送队列行正在重排时显示的共享插入指示高度。
const COMPOSER_PENDING_QUEUE_DROP_INDICATOR_HEIGHT: f32 = 8.0;

/// Shared fixed row height used by each enabled AI capability target row inside settings pages.
/// 设置页中每一条已启用 AI 能力目标行使用的共享固定行高。
const AI_CAPABILITY_ROUTE_TARGET_ROW_HEIGHT: f32 = 46.0;

/// Shared indicator height shown while one enabled AI capability target row is being reordered.
/// 当某条已启用 AI 能力目标行正在重排时显示的共享插入指示高度。
const AI_CAPABILITY_ROUTE_DROP_INDICATOR_HEIGHT: f32 = 8.0;

/// Shared square size used by the fixed send-or-stop button beside the main prompt composer.
/// 主提示词编辑器右侧固定发送或停止按钮使用的共享方形尺寸。
const COMPOSER_TRIGGER_BUTTON_SIZE: f32 = COMPOSER_CONTROL_ROW_HEIGHT;

/// Shared icon size used by the trailing circular send-or-stop button.
/// 尾部圆形发送或停止按钮使用的共享图标尺寸。
const COMPOSER_TRIGGER_BUTTON_ICON_SIZE: f32 = 18.0;

/// Shared width used by compact workbench context menus such as the composer menu.
/// 输入区等紧凑工作台右键菜单使用的共享宽度。
const WORKBENCH_CONTEXT_MENU_COMPACT_WIDTH: f32 = 168.0;

/// Shared width used by regular workbench context menus such as project and transcript menus.
/// 项目与转录等常规工作台右键菜单使用的共享宽度。
const WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH: f32 = 200.0;

/// Shared row height used by unified workbench context-menu actions.
/// 统一工作台右键菜单动作项使用的共享行高。
const WORKBENCH_CONTEXT_MENU_ROW_HEIGHT: f32 = 34.0;

/// Shared maximal undo snapshot count retained for the composer editor.
/// 提示词编辑器保留的共享最大撤销快照数量。
const COMPOSER_UNDO_HISTORY_LIMIT: usize = 64;

/// Shared fixed width used by the compact execution-mode selector under the main prompt composer.
/// 主提示词编辑器下方紧凑执行模式选择器使用的共享固定宽度。
const WORKBENCH_EXECUTION_MODE_SELECTOR_WIDTH: f32 = 66.0;

/// Shared fixed width used by the inline workbench model trigger surface.
/// 工作台内联模型触发表面使用的共享固定宽度。
const WORKBENCH_MODEL_SELECTOR_WIDTH: f32 = 200.0;

/// Shared fixed width used by the compact workbench web-search selector placed beside the model trigger.
/// 放置在模型触发器旁侧的紧凑工作台网页搜索选择器使用的共享固定宽度。
const WORKBENCH_AI_CAPABILITY_QUICK_CONTROL_WIDTH: f32 = 150.0;

/// Shared fixed width used by the compact reasoning-effort selector placed before the send button.
/// 放置在发送按钮之前的紧凑推理级别选择器使用的共享固定宽度。
const WORKBENCH_REASONING_SELECTOR_WIDTH: f32 = 112.0;

/// Shared fixed width used by the in-app workbench model-picker modal card.
/// 工作台内嵌模型选择弹窗卡片使用的共享固定宽度。
const WORKBENCH_MODEL_PICKER_MODAL_WIDTH: f32 = 680.0;

/// Shared fixed width used by the reusable blocking notice modal card.
/// 可复用阻塞提示弹窗卡片使用的共享固定宽度。
const WORKBENCH_NOTICE_MODAL_WIDTH: f32 = 460.0;

/// Shared fixed height used by the reusable blocking notice modal card.
/// 可复用阻塞提示弹窗卡片使用的共享固定高度。
const WORKBENCH_NOTICE_MODAL_HEIGHT: f32 = 248.0;

/// Shared fixed height used by the reusable blocking notice modal header.
/// 可复用阻塞提示弹窗标题栏使用的共享固定高度。
const WORKBENCH_NOTICE_MODAL_HEADER_HEIGHT: f32 = 36.0;

/// Shared fixed width used by each reusable blocking notice modal action button.
/// 可复用阻塞提示弹窗每个操作按钮使用的共享固定宽度。
const WORKBENCH_NOTICE_MODAL_ACTION_BUTTON_WIDTH: f32 = 118.0;

/// Ratio of the main window reserved by the blocking image-preview modal card.
/// 阻塞式图片预览弹窗卡片相对于主窗口保留的尺寸比例。
const WORKBENCH_IMAGE_PREVIEW_MODAL_RATIO: f32 = 0.70;

/// Minimum width kept for the blocking image-preview modal card on narrow windows.
/// 窄窗口下阻塞式图片预览弹窗卡片保留的最小宽度。
const WORKBENCH_IMAGE_PREVIEW_MODAL_MIN_WIDTH: f32 = 520.0;

/// Minimum height kept for the blocking image-preview modal card on short windows.
/// 矮窗口下阻塞式图片预览弹窗卡片保留的最小高度。
const WORKBENCH_IMAGE_PREVIEW_MODAL_MIN_HEIGHT: f32 = 360.0;

/// Horizontal padding used inside the blocking image-preview modal card.
/// 阻塞式图片预览弹窗卡片内部使用的水平内边距。
const WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_X: f32 = 20.0;

/// Vertical padding used inside the blocking image-preview modal card.
/// 阻塞式图片预览弹窗卡片内部使用的垂直内边距。
const WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_Y: f32 = 18.0;

/// Vertical spacing kept between the image-preview header and the preview surface.
/// 图片预览标题栏与预览区域之间保留的垂直间距。
const WORKBENCH_IMAGE_PREVIEW_BODY_SPACING: f32 = 10.0;

/// Shared fixed height used by the compact image-preview modal header.
/// 紧凑图片预览弹窗标题栏使用的共享固定高度。
const WORKBENCH_IMAGE_PREVIEW_HEADER_HEIGHT: f32 = 40.0;

/// Default image preview zoom factor used when opening or resetting the preview.
/// 打开或还原图片预览时使用的默认缩放倍率。
const WORKBENCH_IMAGE_PREVIEW_DEFAULT_ZOOM: f32 = 1.0;

/// Minimum image preview zoom factor accepted by wheel and button controls.
/// 滚轮与按钮控制允许的最小图片预览缩放倍率。
const WORKBENCH_IMAGE_PREVIEW_MIN_ZOOM: f32 = 0.25;

/// Maximum image preview zoom factor accepted by wheel and button controls.
/// 滚轮与按钮控制允许的最大图片预览缩放倍率。
const WORKBENCH_IMAGE_PREVIEW_MAX_ZOOM: f32 = 4.0;

/// Image preview zoom delta applied by toolbar buttons.
/// 图片预览工具栏按钮应用的缩放步长。
const WORKBENCH_IMAGE_PREVIEW_BUTTON_ZOOM_STEP: f32 = 0.25;

/// Image preview zoom delta applied for each wheel-scroll notch.
/// 每个滚轮滚动刻度应用的图片预览缩放步长。
const WORKBENCH_IMAGE_PREVIEW_WHEEL_ZOOM_STEP: f32 = 0.10;

/// Pixel scroll amount applied for each line-based wheel notch inside the image preview.
/// 图片预览内部每个按行滚轮刻度转换出的像素滚动量。
const WORKBENCH_IMAGE_PREVIEW_WHEEL_SCROLL_LINE_STEP: f32 = 60.0;

/// Pixel movement below this threshold is ignored while left-button panning the image preview.
/// 左键平移图片预览时低于该阈值的像素移动会被忽略。
const WORKBENCH_IMAGE_PREVIEW_PAN_EPSILON: f32 = 0.5;

/// Shared fixed height used by the scrollable result list inside the workbench model-picker modal.
/// 工作台模型选择弹窗内部滚动结果列表使用的共享固定高度。
const WORKBENCH_MODEL_PICKER_LIST_HEIGHT: f32 = 340.0;

/// Shared fixed row height used by each virtualized model-picker option.
/// 工作台模型选择弹窗中每个虚拟化选项使用的共享固定行高。
const WORKBENCH_MODEL_PICKER_ROW_HEIGHT: f32 = 40.0;

/// Shared vertical gap used between two neighboring virtualized model-picker rows.
/// 工作台模型选择弹窗中相邻虚拟化选项之间使用的共享纵向间隔。
const WORKBENCH_MODEL_PICKER_ROW_GAP: f32 = 6.0;

/// Shared overscan row count kept above and below the visible model-picker viewport.
/// 工作台模型选择弹窗在可视区域上下方保留的共享预加载行数。
const WORKBENCH_MODEL_PICKER_VIRTUAL_OVERSCAN_ROWS: usize = 10;

/// Shared fixed width used by the in-app workspace-history search modal card.
/// 工作台历史搜索弹窗卡片使用的共享固定宽度。
const WORKSPACE_HISTORY_SEARCH_MODAL_WIDTH: f32 = 760.0;

/// Shared fixed height used by the scrollable result list inside the workspace-history search modal.
/// 工作台历史搜索弹窗内部滚动结果列表使用的共享固定高度。
const WORKSPACE_HISTORY_SEARCH_LIST_HEIGHT: f32 = 360.0;

/// Shared fixed width used by the project settings modal card.
/// 项目设置弹窗卡片使用的共享固定宽度。
const TASK_PROJECT_SETTINGS_MODAL_WIDTH: f32 = 640.0;

/// Shared fixed height used by the optional project-scope picker list inside the workspace-history search modal.
/// 工作台历史搜索弹窗内可选项目范围列表使用的共享固定高度。
const WORKSPACE_HISTORY_SEARCH_SCOPE_LIST_HEIGHT: f32 = 220.0;

/// Returns the inner text width available to the main prompt composer after subtracting the editor padding.
/// 返回主提示词编辑器在扣除内部边距之后可用于文本排版的实际宽度。
fn composer_available_text_width(surface_width: f32) -> f32 {
    (surface_width - COMPOSER_EDITOR_HORIZONTAL_INSET).max(COMPOSER_EDITOR_TEXT_SIZE)
}

/// Builds the stable scrollable identifier used by the image preview modal panning surface.
/// 构建图片预览弹窗平移区域使用的稳定滚动容器标识。
fn workbench_image_preview_scrollable_id() -> iced::widget::Id {
    iced::widget::Id::new("workbench-image-preview-scrollable")
}

/// Builds the stable scrollable identifier used by the virtualized workbench model-picker list.
/// 构建工作台虚拟化模型选择列表使用的稳定滚动容器标识。
fn workbench_model_picker_scrollable_id() -> iced::widget::Id {
    iced::widget::Id::new("workbench-model-picker-scrollable")
}

/// Measures the current composer content with the native editor layout engine and returns its minimum bounds plus caret position.
/// 使用原生编辑器布局引擎测量当前提示词内容，并返回其最小边界与光标位置。
fn measure_composer_layout(
    buffer: &text_editor::Content,
    surface_width: f32,
) -> (iced::Size, iced::Point) {
    buffer.measure_plain_text_layout(
        iced::Size::new(composer_available_text_width(surface_width), 100_000.0),
        theme::ui_font(),
        Pixels(COMPOSER_EDITOR_TEXT_SIZE),
        iced::widget::text::LineHeight::Absolute(Pixels(COMPOSER_EDITOR_LINE_HEIGHT)),
        iced::advanced::text::Wrapping::WordOrGlyph,
        Some(1.0),
    )
}

/// Returns the visible line count of one composer buffer measured by the native editor layout engine.
/// 返回由原生编辑器布局引擎测量得到的提示词编辑器可见行数量。
fn composer_visible_line_count(buffer: &text_editor::Content, surface_width: f32) -> usize {
    let text_height = composer_measured_text_height(buffer, surface_width);

    (text_height / COMPOSER_EDITOR_LINE_HEIGHT).ceil().max(1.0) as usize
}

/// Returns the measured text height of one composer buffer using the native editor layout engine.
/// 返回由原生编辑器布局引擎测量得到的提示词编辑器文本高度。
fn composer_measured_text_height(buffer: &text_editor::Content, surface_width: f32) -> f32 {
    let (min_bounds, _) = measure_composer_layout(buffer, surface_width);
    min_bounds.height.max(COMPOSER_EDITOR_LINE_HEIGHT)
}

/// Returns the internal scroll target required to keep the measured composer caret visible.
/// 返回用于让测量得到的提示词光标保持可见所需的内部滚动目标。
fn composer_caret_scroll_target(
    caret_top: f32,
    current_scroll_offset: f32,
    viewport_text_height: f32,
    max_scroll_offset: f32,
) -> f32 {
    let caret_bottom = caret_top + COMPOSER_EDITOR_LINE_HEIGHT;
    let current_viewport_bottom = current_scroll_offset + viewport_text_height;

    if caret_top < current_scroll_offset {
        caret_top.clamp(0.0, max_scroll_offset)
    } else if caret_bottom > current_viewport_bottom {
        (caret_bottom - viewport_text_height).clamp(0.0, max_scroll_offset)
    } else {
        current_scroll_offset.clamp(0.0, max_scroll_offset)
    }
}

/// Returns whether one composer text payload contains any submittable non-whitespace content.
/// 返回某段提示词文本是否包含任何可提交的非空白内容。
fn composer_text_has_submittable_content(text: &str) -> bool {
    !text.trim().is_empty()
}

/// Returns whether one composer payload contains any dispatchable text or attachment input.
/// 返回某条提示词输入载荷是否包含可分发的文本或附件输入。
fn composer_input_has_submittable_content(
    text: &str,
    attachments: &[ConversationAttachment],
) -> bool {
    let parts = build_composer_input_parts(text, attachments);
    composer_parts_have_submittable_content(&parts)
}

/// Builds the low-priority legacy maintenance queue from visible and archived sessions.
/// 根据可见会话与归档会话构建低优先级旧数据维护队列。
fn build_background_legacy_maintenance_queue(
    sessions: &[SessionSummary],
    archived_sessions: &[SessionSummary],
) -> VecDeque<SessionId> {
    let mut seen = BTreeSet::new();
    sessions
        .iter()
        .chain(archived_sessions.iter())
        .filter_map(|session| {
            if session.id.is_nil() || !seen.insert(session.id) {
                None
            } else {
                Some(session.id)
            }
        })
        .collect()
}

/// Returns whether the given composer selection payload contains any effective text.
/// 返回给定提示词选区载荷是否包含任何有效文本。
fn composer_selection_has_content(selection: Option<&str>) -> bool {
    selection.is_some_and(|text| !text.is_empty())
}

/// Prunes the oldest messages from one in-memory transcript slice and returns removed identifiers.
/// 从一段内存转录切片中裁剪最旧消息，并返回被移除的消息标识。
fn prune_session_messages_to_limit(
    messages: &mut Vec<SessionMessageEntry>,
    limit: usize,
) -> Vec<String> {
    if messages.len() <= limit {
        return Vec::new();
    }

    let remove_count = messages.len().saturating_sub(limit);
    messages
        .drain(0..remove_count)
        .map(|message| message.id)
        .collect()
}

/// Merges tool-call and tool-result transcript rows into one human-facing display card while keeping the raw session records untouched.
/// 在保持原始会话记录不变的前提下，将工具调用与工具返回转录行合并为面向人类的一张显示卡片。
fn merge_tool_display_messages(messages: Vec<SessionMessageEntry>) -> Vec<SessionMessageEntry> {
    let mut merged_messages = Vec::with_capacity(messages.len());
    let mut tool_message_indices = BTreeMap::new();

    for message in messages {
        let Some(tool_call_id) = tool_display_message_call_id(&message) else {
            merged_messages.push(message);
            continue;
        };

        match message.record_kind {
            SessionMessageRecordKind::ToolCallOutput => {
                let normalized = normalize_tool_display_message(message);
                tool_message_indices.insert(tool_call_id, merged_messages.len());
                merged_messages.push(normalized);
            }
            SessionMessageRecordKind::ToolResultOutput => {
                let normalized = normalize_tool_display_message(message);
                if let Some(index) = tool_message_indices.get(&tool_call_id).copied() {
                    merge_tool_result_into_display_entry(&mut merged_messages[index], normalized);
                } else {
                    tool_message_indices.insert(tool_call_id, merged_messages.len());
                    merged_messages.push(normalized);
                }
            }
            _ => merged_messages.push(message),
        }
    }

    merged_messages
}

/// Merges one freshly projected conversation-state slice back into the already hydrated visible transcript window so older on-demand history rows are preserved instead of disappearing during live updates.
/// 将一段新鲜投影出的 conversation-state 切片重新合并回已经水合的可见转录窗口，确保按需加载的旧历史行不会在实时更新期间消失。
fn merge_projected_session_messages_into_existing_window(
    existing_messages: &[SessionMessageEntry],
    projected_messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    if existing_messages.is_empty() {
        return projected_messages;
    }
    if projected_messages.is_empty() {
        return existing_messages.to_vec();
    }

    let projected_by_id = projected_messages
        .iter()
        .cloned()
        .map(|message| (message.id.clone(), message))
        .collect::<BTreeMap<_, _>>();
    let mut consumed_projected_ids = BTreeSet::new();
    let mut merged_messages =
        Vec::with_capacity(existing_messages.len().max(projected_messages.len()));

    for existing_message in existing_messages {
        if let Some(projected_message) = projected_by_id.get(&existing_message.id).cloned() {
            consumed_projected_ids.insert(existing_message.id.clone());
            merged_messages.push(projected_message);
        } else {
            merged_messages.push(existing_message.clone());
        }
    }

    for projected_message in projected_messages {
        if consumed_projected_ids.contains(&projected_message.id) {
            continue;
        }
        merged_messages.push(projected_message);
    }

    merged_messages
}

/// Returns whether one transcript message is a tool-detail row whose heavy body should stay out of the main UI projection.
/// 返回一条转录消息是否为工具明细行，这类行的重型正文不应进入主界面投影。
fn session_message_is_tool_detail(message: &SessionMessageEntry) -> bool {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
}

/// Builds the lightweight main-transcript projection for one tool-detail row while preserving its display metadata.
/// 为单条工具明细行构建轻量主转录投影，同时保留展示所需元数据。
fn compact_tool_detail_message_for_projection(
    message: &SessionMessageEntry,
) -> SessionMessageEntry {
    if !session_message_is_tool_detail(message) {
        return message.clone();
    }

    SessionMessageEntry {
        id: message.id.clone(),
        kind: message.kind,
        record_kind: message.record_kind,
        visible: message.visible,
        title: message.title.clone(),
        body: String::new(),
        metadata: message
            .metadata
            .as_ref()
            .and_then(compact_tool_detail_metadata_for_projection),
        created_at: message.created_at.clone(),
    }
}

/// Builds compact metadata for a visible tool card without carrying raw tool payloads through the render tree.
/// 为可见工具卡构建紧凑元数据，避免原始工具载荷穿过渲染树。
fn compact_tool_detail_metadata_for_projection(metadata: &Value) -> Option<Value> {
    let source = metadata.as_object()?;
    let mut compact = serde_json::Map::new();

    for key in [
        "tool_display_state",
        "tool_call_id",
        "tool_result_id",
        "tool_name",
        "name",
        "title",
        "display_title",
        "kind",
        "status",
        "execution_status",
        "run_state",
        "task_id",
        "session_id",
        "parent_session_id",
        "public_id",
    ] {
        insert_compact_metadata_field(&mut compact, source, key);
    }

    if let Some(payload) = source.get("payload").and_then(Value::as_object)
        && let Some(compact_payload) = compact_task_session_payload_for_projection(payload)
    {
        compact.insert(String::from("payload"), compact_payload);
    }

    if let Some(arguments) = source.get("arguments").and_then(Value::as_object)
        && let Some(compact_arguments) = compact_task_arguments_for_projection(arguments)
    {
        compact.insert(String::from("arguments"), compact_arguments);
    }

    if let Some(host_result) = resolved_host_result_from_tool_metadata(metadata)
        .and_then(compact_generated_host_result_for_projection)
    {
        compact.insert(String::from("host_result"), host_result);
    }

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Copies one small metadata field into the compact projection when it exists.
/// 当小型元数据字段存在时，将其复制到紧凑投影中。
fn insert_compact_metadata_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

/// Builds the task-session payload subset needed by cards and subagent navigation.
/// 构建卡片与子代理导航所需的 task-session 载荷子集。
fn compact_task_session_payload_for_projection(
    payload: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut compact = serde_json::Map::new();
    for key in [
        "task_session_id",
        "task_id",
        "display_title",
        "execution_status",
        "description",
        "subagent",
    ] {
        insert_compact_metadata_field(&mut compact, payload, key);
    }
    insert_compact_preview_string_field(&mut compact, payload, "prompt", 240);
    insert_compact_preview_string_field(&mut compact, payload, "result", 240);

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Builds the in-flight task arguments subset used before a task-session result exists.
/// 构建 task-session 结果出现前用于展示的进行中任务参数子集。
fn compact_task_arguments_for_projection(
    arguments: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut compact = serde_json::Map::new();
    for key in ["task_id", "description", "subagent"] {
        insert_compact_metadata_field(&mut compact, arguments, key);
    }
    insert_compact_preview_string_field(&mut compact, arguments, "prompt", 240);

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Copies one string field with a Unicode-safe preview cap into the compact projection.
/// 将一个字符串字段按 Unicode 安全预览上限复制到紧凑投影中。
fn insert_compact_preview_string_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    key: &str,
    max_chars: usize,
) {
    let Some(value) = source
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let compact_value =
        crate::agent_runtime::provider_stream_projection::truncate_preview(value, max_chars);
    target.insert(key.to_string(), Value::String(compact_value));
}

/// Preserves only generated-media host-result fields needed by inline cards and modal actions.
/// 仅保留内联卡片与弹层动作所需的生成媒体宿主结果字段。
fn compact_generated_host_result_for_projection(host_result: &Value) -> Option<Value> {
    if host_result.get("kind").and_then(Value::as_str) != Some("generated_attachments") {
        return None;
    }
    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let compact_outputs = outputs
        .iter()
        .filter_map(compact_generated_output_for_projection)
        .collect::<Vec<_>>();
    if compact_outputs.is_empty() {
        return None;
    }

    Some(json!({
        "kind": "generated_attachments",
        "payload": {
            "outputs": compact_outputs,
        },
    }))
}

/// Preserves the small generated-output descriptor required to render and open produced media.
/// 保留渲染和打开生成媒体所需的小型生成输出描述符。
fn compact_generated_output_for_projection(output: &Value) -> Option<Value> {
    let source = output.as_object()?;
    let mut compact = serde_json::Map::new();
    for key in [
        "kind",
        "file_name",
        "local_path",
        "mime_type",
        "byte_len",
        "preview_local_path",
        "asset_id",
    ] {
        insert_compact_metadata_field(&mut compact, source, key);
    }

    (!compact.is_empty()).then_some(Value::Object(compact))
}

/// Builds the visible transcript slice that should participate in one background agent-view projection refresh.
/// 构建应参与单次后台 agent-view 投影刷新任务的可见转录切片。
fn visible_session_messages_for_agent_view_projection(
    messages: Vec<SessionMessageEntry>,
    floating_thinking_message_id: Option<&str>,
) -> Vec<SessionMessageEntry> {
    merge_tool_display_messages(
        messages
            .into_iter()
            .filter(|message| {
                message.is_visible_to_user()
                    && floating_thinking_message_id.is_none_or(|active_id| message.id != active_id)
            })
            .collect(),
    )
}

/// Builds one session-scoped unified transcript projection snapshot entirely off the UI thread.
/// 在 UI 线程之外完整构建一份按会话归属的统一转录投影视图快照。
fn build_session_agent_view_projection_snapshot(
    session_id: SessionId,
    messages: Vec<SessionMessageEntry>,
    runtime_events: Vec<RuntimeEvent>,
) -> SessionAgentViewProjectionSnapshot {
    let document = build_agent_view_document(&messages, &runtime_events);
    let selectable_document =
        vulcan_agent_view::AgentViewSelectableDocument::from_document(&document);
    let selectable_text = selectable_document.buffer().to_string();
    let raw_source_text = agent_view_document_raw_source(&document);

    SessionAgentViewProjectionSnapshot {
        session_id,
        document,
        selectable_document,
        selectable_text,
        raw_source_text,
    }
}

/// Extracts the stable tool-call identifier from one display-capable transcript message when metadata provides it.
/// 当元数据中存在稳定工具调用标识时，从单条可显示转录消息中提取该标识。
fn tool_display_message_call_id(message: &SessionMessageEntry) -> Option<String> {
    matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
    .then_some(())?;
    message
        .metadata
        .as_ref()?
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Returns the structured task-session payload attached to one tool-display metadata object when the tool result belongs to the host-managed `task` surface.
/// 当工具结果属于宿主管理的 `task` 表面时，返回其工具显示元数据对象中附带的结构化 task-session 载荷。
fn task_session_payload_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    (metadata.get("kind").and_then(Value::as_str) == Some("task_session")).then_some(())?;
    metadata.get("payload")
}

/// Returns the caller-facing task identifier stored inside one task-session payload when available.
/// 返回 task 会话载荷中保存的调用方面向任务标识。
fn task_session_public_id_from_payload(payload: &Value) -> Option<&str> {
    payload
        .get("task_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns the stable human-readable task title stored inside one task-session payload when available.
/// 返回 task 会话载荷中保存的稳定人类可读任务标题。
fn task_session_display_title_from_payload(payload: &Value) -> Option<&str> {
    payload
        .get("display_title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Returns the internal child-session identifier stored inside one task-session payload, falling back to legacy payloads that only persisted `task_id`.
/// 返回 task 会话载荷中保存的内部子会话标识；若旧版本只持久化了 `task_id`，则回退兼容解析。
fn task_session_internal_id_from_payload(payload: &Value) -> Option<SessionId> {
    payload
        .get("task_session_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("task_id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Returns the original `task` tool arguments attached to one tool-display metadata object when the card is still in flight.
/// 当卡片仍处于进行中状态时，返回其工具显示元数据对象中附带的原始 `task` 工具参数。
fn task_session_arguments_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    (metadata.get("tool_name").and_then(Value::as_str) == Some("task")).then_some(())?;
    metadata.get("arguments")
}

/// Returns the best-effort caller-facing task identifier attached to one task card from either persisted payload metadata or in-flight tool arguments.
/// 从持久化载荷元数据或进行中工具参数中返回一条最佳努力的调用方面向任务标识。
fn task_session_declared_task_id_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_public_id_from_payload)
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("task_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

/// Returns the effective execution status stored in one task-session metadata object, falling back to display-only state when no payload exists yet.
/// 返回一份 task-session 元数据对象中记录的有效执行状态；若尚无 payload，则回退到仅显示层状态。
fn task_session_execution_status_from_tool_metadata<'a>(metadata: &'a Value) -> Option<&'a str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("execution_status"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            metadata
                .get("tool_display_state")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session prompt text extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 提示词文本。
fn task_session_prompt_text_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("prompt"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("prompt"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session description extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 描述文本。
fn task_session_description_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("description"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("description"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one trimmed task-session subagent label extracted from either persisted payload metadata or in-flight tool arguments.
/// 从持久化 payload 元数据或进行中工具参数中提取一段裁剪后的 task-session 子代理标签。
fn task_session_subagent_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(|payload| payload.get("subagent"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            task_session_arguments_from_tool_metadata(metadata)
                .and_then(|arguments| arguments.get("subagent"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
}

/// Returns one stable human-readable task title extracted from persisted payload metadata when present.
/// 当存在时，从持久化 payload 元数据中返回一条稳定的人类可读任务标题。
fn task_session_display_title_from_tool_metadata(metadata: &Value) -> Option<&str> {
    task_session_payload_from_tool_metadata(metadata)
        .and_then(task_session_display_title_from_payload)
}

/// Builds the Markdown summary shown by the legacy tool modal when only a persisted task-session result remains available.
/// 当仅剩持久化 task-session 结果可用时，构建旧工具弹层展示的 Markdown 摘要。
fn task_session_modal_markdown_from_tool_metadata(metadata: &Value) -> Option<String> {
    let payload = task_session_payload_from_tool_metadata(metadata)?;
    if let Some(result_text) = payload
        .get("result")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(result_text.to_string());
    }

    match task_session_execution_status_from_tool_metadata(metadata) {
        Some("stopped") => Some(String::from(subagent_task_stopped_summary_text())),
        Some("failed") => Some(String::from(
            "子代理调用已失败，当前仅保留最后一次错误摘要。",
        )),
        _ => None,
    }
}

/// Returns the canonical user-facing stop summary reused by subagent cards, panels, and legacy tool-modal fallbacks.
/// 返回供子代理卡片、面板与旧工具弹层回退共同复用的规范用户可见停止摘要。
fn subagent_task_stopped_summary_text() -> &'static str {
    "子代理已按用户请求主动停止，当前没有完整返回结果。请基于现有上下文决定是否重试、改写任务，或直接继续主流程。"
}

/// Returns the emoji-prefixed UI badge text for one subagent execution-status string.
/// 返回单条子代理 execution-status 字符串对应的 emoji 前缀界面徽标文案。
fn subagent_execution_status_badge_label(execution_status: &str) -> &'static str {
    match execution_status {
        "running" => "🔄 运行中",
        "stopped" => "⏹️ 已停止",
        "failed" => "❌ 失败",
        "completed" => "✅ 完成",
        _ => "✅ 完成",
    }
}

/// Returns the emoji-prefixed UI badge text for one subagent run-state value.
/// 返回单个子代理运行状态对应的 emoji 前缀界面徽标文案。
fn subagent_run_state_badge_label(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "🔄 运行中",
        SubagentTaskRunState::Stopped => "⏹️ 已停止",
        SubagentTaskRunState::Completed => "✅ 完成",
        SubagentTaskRunState::Failed => "❌ 失败",
    }
}

/// Builds one compact human-facing task-session label from the optional task description and subagent name.
/// 根据可选任务描述与子代理名称构建一条紧凑的人类可读 task-session 标签。
fn task_session_tool_display_label(
    display_title: Option<&str>,
    description: Option<&str>,
    subagent: Option<&str>,
    public_task_id: Option<&str>,
    prompt: Option<&str>,
) -> String {
    if let Some(display_title) = display_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return display_title.to_string();
    }
    let description = description.map(str::trim).filter(|value| !value.is_empty());
    let subagent = subagent.map(str::trim).filter(|value| !value.is_empty());
    let public_task_id = public_task_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (description, subagent) {
        (Some(description), Some(subagent)) if description != subagent => {
            return format!("{description}（{subagent}）");
        }
        (Some(description), _) => return description.to_string(),
        (_, Some(subagent)) => return subagent.to_string(),
        _ => {}
    }

    if let Some(public_task_id) = public_task_id {
        if SessionId::parse_str(public_task_id).is_err() {
            return public_task_id.to_string();
        }
    }
    if let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) {
        return pending_prompt_preview_text(prompt, &[]);
    }
    if let Some(public_task_id) = public_task_id {
        return public_task_id.to_string();
    }
    String::from("子代理")
}

/// Resolves the localized task-session status copy shown inside the dedicated `task` transcript card title.
/// 解析展示在专属 `task` 转录卡片标题中的本地化 task-session 状态文案。
fn task_session_tool_display_status(
    record_kind: SessionMessageRecordKind,
    metadata: &Value,
) -> &'static str {
    if let Some(execution_status) = task_session_execution_status_from_tool_metadata(metadata) {
        return subagent_execution_status_badge_label(execution_status);
    }

    match record_kind {
        SessionMessageRecordKind::ToolResultOutput => "✅ 完成",
        _ => "🔄 运行中",
    }
}

/// Builds the dedicated task-session title when one tool-display metadata object belongs to the `task` surface.
/// 当某个工具显示元数据对象属于 `task` 表面时，构建其专属 task-session 标题。
fn task_session_tool_display_title(
    record_kind: SessionMessageRecordKind,
    metadata: &Value,
) -> Option<String> {
    let label = if let Some(payload) = task_session_payload_from_tool_metadata(metadata) {
        task_session_tool_display_label(
            task_session_display_title_from_payload(payload),
            payload.get("description").and_then(Value::as_str),
            payload.get("subagent").and_then(Value::as_str),
            task_session_public_id_from_payload(payload),
            payload.get("prompt").and_then(Value::as_str),
        )
    } else if let Some(arguments) = task_session_arguments_from_tool_metadata(metadata) {
        task_session_tool_display_label(
            task_session_display_title_from_tool_metadata(metadata),
            arguments.get("description").and_then(Value::as_str),
            arguments.get("subagent").and_then(Value::as_str),
            arguments.get("task_id").and_then(Value::as_str),
            arguments.get("prompt").and_then(Value::as_str),
        )
    } else {
        return None;
    };
    let status = task_session_tool_display_status(record_kind, metadata);
    Some(format!("调度 {label} {status}"))
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的一条紧凑单行预览文本。
fn pending_prompt_preview_text(prompt: &str, attachments: &[ConversationAttachment]) -> String {
    let parts = build_composer_input_parts(prompt, attachments);
    composer_parts_preview_text(&parts, 48usize)
}

/// Builds one full single-line label used by the visible pending-prompt queue panel.
/// 构建显示在可见待发送队列面板中的完整单行标签文本。
fn pending_prompt_panel_label_text(prompt: &str, attachments: &[ConversationAttachment]) -> String {
    let parts = build_composer_input_parts(prompt, attachments);
    composer_parts_label_text(&parts)
}

/// Builds one transcript-visible body that keeps user text and attachment names together.
/// 构建一段会在转录中可见的正文，同时保留用户文本与附件名称。
fn build_user_prompt_transcript_body(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> String {
    let visible_attachments = attachments
        .iter()
        .filter(|attachment| attachment.kind != ConversationAttachmentKind::Image)
        .cloned()
        .collect::<Vec<_>>();
    let parts = build_composer_input_parts(prompt, &visible_attachments);
    let body = composer_parts_transcript_body(&parts);

    if prompt.trim().is_empty() && !attachments.is_empty() && visible_attachments.is_empty() {
        String::new()
    } else {
        body
    }
}

/// Builds transcript-only metadata for one visible user prompt so GUI projections can recover canonical text and attachment strips after reload.
/// 为一条可见用户提示词构建仅供转录使用的元数据，使图形界面投影能在重开后恢复规范文本与附件面板。
fn build_user_prompt_transcript_metadata(
    prompt: &str,
    attachments: &[ConversationAttachment],
) -> Option<Value> {
    if attachments.is_empty() {
        return None;
    }

    let serialized_attachments = serde_json::to_value(attachments).ok()?;
    Some(json!({
        "canonical_user_text": prompt,
        "transcript_attachments": serialized_attachments,
    }))
}

/// Returns whether pending-prompt dispatch must stay blocked because Anthropic sessions cannot safely continue the parent coordinator while detached subagents from the previous tool round are still running.
/// 返回待发送提示词分发是否必须继续保持阻塞，因为 Anthropic 会话在前一轮工具回合的分离子代理仍在运行时，无法安全地让父协调器继续推进。
fn anthropic_pending_prompt_dispatch_is_blocked(
    uses_anthropic_runtime: bool,
    has_running_subagents: bool,
) -> bool {
    uses_anthropic_runtime && has_running_subagents
}

/// Returns the internal queue index that should dispatch next, optionally preferring delegated-completion callbacks so Anthropic sessions can close older tool rounds before user-authored follow-ups resume.
/// 返回内部队列下一条应被分发的索引；在需要时优先选择子代理完成回流项，使 Anthropic 会话能先闭合旧工具回合，再恢复处理用户跟进消息。
fn pending_prompt_dispatch_index(
    queue: &[PendingSessionPromptEntry],
    prefer_delegate_completion: bool,
) -> usize {
    if prefer_delegate_completion
        && let Some(index) = queue
            .iter()
            .position(|entry| entry.source == PendingSessionPromptSource::DelegateCompletion)
    {
        return index;
    }
    0
}

/// Resolves the localized terminal status label injected into delegated completion follow-up prompts.
/// 解析注入到委派完成跟进提示词中的本地化终态标签。
fn delegate_completion_status_label(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "仍在进行中",
        SubagentTaskRunState::Completed => "已完成",
        SubagentTaskRunState::Stopped => "已停止",
        SubagentTaskRunState::Failed => "已失败",
    }
}

/// Builds one queue-strip preview line for a delegated subagent completion callback.
/// 为一次委派子代理完成回调构建队列条预览文案。
fn build_delegate_completion_followup_preview(state: &SubagentTaskPanelState) -> String {
    format!(
        "子代理 {} {}",
        state.subagent_label,
        delegate_completion_status_label(state.run_state)
    )
}

/// Builds one synthetic follow-up prompt that tells the parent coordinator a delegated subagent has reported back.
/// 构建一条合成跟进提示词，用于告知父协调代理某个委派子代理已经回报结果。
fn build_delegate_completion_followup_prompt(state: &SubagentTaskPanelState) -> String {
    let result_text = state.result_text.trim();
    let safe_result = if result_text.is_empty() {
        "当前没有更多结构化返回内容，请根据任务状态继续主流程。"
    } else {
        result_text
    };
    format!(
        "<system-reminder>\n子代理“{}”已{}。\n任务标识：{}。\n\n子代理返回内容：\n{}\n\n请立即吸收这条结果，并继续推进主流程；如果还有其他子代理仍在执行，请同时维持协调与收敛。\n</system-reminder>",
        state.subagent_label,
        delegate_completion_status_label(state.run_state),
        state.public_task_id,
        safe_result
    )
}

/// Returns whether one visible tool transcript message belongs to the dedicated host-managed `task` surface.
/// 返回某条可见工具转录消息是否属于宿主管理的专属 `task` 表面。
fn tool_display_message_is_task_session(message: &SessionMessageEntry) -> bool {
    message.metadata.as_ref().is_some_and(|metadata| {
        task_session_payload_from_tool_metadata(metadata).is_some()
            || task_session_arguments_from_tool_metadata(metadata).is_some()
    })
}

/// Parses the generated-attachments host payload attached to one tool-display metadata object into managed conversation attachments.
/// 将附着在工具显示元数据对象上的 generated-attachments 宿主载荷解析为托管会话附件集合。
fn generated_attachments_from_tool_metadata(metadata: &Value) -> Vec<ConversationAttachment> {
    let Some(host_result) = resolved_host_result_from_tool_metadata(metadata) else {
        return Vec::new();
    };
    if host_result.get("kind").and_then(Value::as_str) != Some("generated_attachments") {
        return Vec::new();
    }
    host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)
        .map(|outputs| {
            outputs
                .iter()
                .filter_map(parse_generated_attachment_output)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Resolves the structured host-result payload stored on one tool-display metadata object, accepting both nested and flattened runtime-event shapes.
/// 解析单份工具显示元数据对象上保存的结构化宿主结果载荷，同时兼容嵌套与扁平两种运行时事件形状。
fn resolved_host_result_from_tool_metadata(metadata: &Value) -> Option<&Value> {
    metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .or_else(|| {
            (metadata.get("kind").is_some() && metadata.get("payload").is_some())
                .then_some(metadata)
        })
}

/// Parses one serialized generated-attachment output object back into the local attachment structure used by the GUI.
/// 将单条序列化生成附件输出对象解析回图形界面使用的本地附件结构。
fn parse_generated_attachment_output(output: &Value) -> Option<ConversationAttachment> {
    let kind = parse_generated_attachment_kind(
        output
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?,
    )?;
    let file_name = output
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let local_path = output
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let mime_type = output
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();

    Some(ConversationAttachment {
        kind,
        file_name,
        local_path,
        source_path: None,
        asset_id: output
            .get("asset_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        byte_len: output.get("byte_len").and_then(Value::as_u64),
        preview_local_path: output
            .get("preview_local_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        mime_type,
    })
}

/// Maps one serialized generated-attachment modality name back to the local attachment-kind enum.
/// 将序列化生成附件模态名称映射回本地附件类型枚举。
fn parse_generated_attachment_kind(value: &str) -> Option<ConversationAttachmentKind> {
    match value {
        "image" => Some(ConversationAttachmentKind::Image),
        "audio" => Some(ConversationAttachmentKind::Audio),
        "video" => Some(ConversationAttachmentKind::Video),
        "pdf" => Some(ConversationAttachmentKind::Pdf),
        _ => None,
    }
}

/// Builds one human-facing generated-media title from the host-produced attachment list stored on a tool card.
/// 根据工具卡片上保存的宿主生成附件列表构建一条面向用户的人类可读标题。
fn generated_attachment_tool_display_title(metadata: &Value) -> Option<String> {
    let attachments = generated_attachments_from_tool_metadata(metadata);
    let first_kind = attachments
        .first()
        .map(|attachment| attachment.kind.clone())?;
    let base_label = match first_kind {
        ConversationAttachmentKind::Image => "已生成图片",
        ConversationAttachmentKind::Audio => "已生成音频",
        ConversationAttachmentKind::Video => "已生成视频",
        ConversationAttachmentKind::Pdf => "已生成文档",
    };
    if attachments.len() <= 1 {
        Some(base_label.to_string())
    } else {
        Some(format!("{base_label}（共 {} 个）", attachments.len()))
    }
}

/// Normalizes one tool display message into the unified transcript-card title expected by the merged human-facing view.
/// 将单条工具显示消息规范化为合并后面向人类视图所需的统一转录卡片标题。
fn normalize_tool_display_message(mut message: SessionMessageEntry) -> SessionMessageEntry {
    if let Some(metadata) = message.metadata.as_ref() {
        if let Some(task_title) = task_session_tool_display_title(message.record_kind, metadata) {
            message.title = task_title;
        } else if let Some(generated_title) = generated_attachment_tool_display_title(metadata) {
            message.title = generated_title;
        } else if let Some(tool_name) = metadata
            .get("tool_name")
            .and_then(Value::as_str)
            .filter(|tool_name| !tool_name.trim().is_empty())
        {
            message.title = format!("调用 {tool_name} 工具");
        }
    }
    message.metadata = attach_tool_display_state_metadata(
        message.metadata.take(),
        tool_display_state_for_record_kind(message.record_kind),
    );
    message
}

/// Merges one completed tool-result display message into the existing tool-call display card shown to the user.
/// 将一条已完成工具结果显示消息合并到当前展示给用户的工具调用显示卡片中。
fn merge_tool_result_into_display_entry(
    existing: &mut SessionMessageEntry,
    tool_result_message: SessionMessageEntry,
) {
    let tool_result_was_task_session = tool_display_message_is_task_session(&tool_result_message);
    let tool_result_title = tool_result_message.title.clone();
    existing.record_kind = SessionMessageRecordKind::ToolCallOutput;
    existing.body = merge_tool_display_body(&existing.body, &tool_result_message.body);
    existing.metadata =
        merge_tool_display_metadata(existing.metadata.take(), tool_result_message.metadata);
    existing.metadata = attach_tool_display_state_metadata(existing.metadata.take(), "completed");
    if let Some(metadata) = existing.metadata.as_ref() {
        if let Some(task_title) = task_session_tool_display_title(existing.record_kind, metadata) {
            existing.title = task_title;
            return;
        }
    }
    if (existing.title.trim().is_empty() || tool_result_was_task_session)
        && !tool_result_title.trim().is_empty()
    {
        existing.title = tool_result_title;
    }
}

/// Combines one tool-call body and one tool-result body into the single Markdown payload shown inside the merged display card.
/// 将工具调用正文与工具结果正文组合成合并显示卡片中的单份 Markdown 载荷。
fn merge_tool_display_body(tool_call_body: &str, tool_result_body: &str) -> String {
    let tool_call_body = tool_call_body.trim();
    let tool_result_body = tool_result_body.trim();
    match (tool_call_body.is_empty(), tool_result_body.is_empty()) {
        (true, true) => String::new(),
        (false, true) => tool_call_body.to_string(),
        (true, false) => tool_result_body.to_string(),
        (false, false) => format!("{tool_call_body}\n\n---\n\n{tool_result_body}"),
    }
}

/// Shallow-merges one tool-call metadata object with the later tool-result metadata object so the display layer can see the full execution snapshot.
/// 将工具调用元数据对象与后续工具结果元数据对象做浅层合并，便于显示层看到完整执行快照。
fn merge_tool_display_metadata(
    tool_call_metadata: Option<Value>,
    tool_result_metadata: Option<Value>,
) -> Option<Value> {
    match (tool_call_metadata, tool_result_metadata) {
        (Some(Value::Object(mut tool_call_object)), Some(Value::Object(tool_result_object))) => {
            let preferred_task_session_payload = match (
                task_session_payload_execution_rank_from_metadata_object(&tool_call_object),
                task_session_payload_execution_rank_from_metadata_object(&tool_result_object),
            ) {
                (Some(call_rank), Some(result_rank)) if call_rank > result_rank => {
                    task_session_payload_from_metadata_object(&tool_call_object).cloned()
                }
                (Some(call_rank), Some(result_rank)) if result_rank > call_rank => {
                    task_session_payload_from_metadata_object(&tool_result_object).cloned()
                }
                _ => None,
            };
            tool_call_object.extend(tool_result_object);
            if let Some(preferred_payload) = preferred_task_session_payload {
                tool_call_object.insert(
                    String::from("kind"),
                    Value::String(String::from("task_session")),
                );
                tool_call_object.insert(String::from("payload"), preferred_payload);
            }
            Some(Value::Object(tool_call_object))
        }
        (Some(metadata), None) => Some(metadata),
        (None, Some(metadata)) => Some(metadata),
        (Some(_), Some(metadata)) => Some(metadata),
        (None, None) => None,
    }
}

/// Returns the nested task-session payload stored inside one tool-display metadata object when available.
/// 当存在时，返回单份工具显示元数据对象中嵌套保存的 task-session payload。
fn task_session_payload_from_metadata_object(
    metadata: &serde_json::Map<String, Value>,
) -> Option<&Value> {
    metadata
        .get("payload")
        .filter(|payload| payload.is_object())
}

/// Returns one comparable execution-rank score for the task-session payload embedded in tool-display metadata.
/// 返回工具显示元数据中嵌入的 task-session payload 的可比较执行阶段优先级分值。
fn task_session_payload_execution_rank_from_metadata_object(
    metadata: &serde_json::Map<String, Value>,
) -> Option<u8> {
    let execution_status = task_session_payload_from_metadata_object(metadata)
        .and_then(|payload| payload.get("execution_status"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(match execution_status {
        "running" => 0,
        "created" => 0,
        "completed" => 2,
        "stopped" => 2,
        "failed" => 2,
        _ => 1,
    })
}

/// Returns the stable display state used by one human-facing tool execution card.
/// 返回单张面向人类的工具执行卡片所使用的稳定显示状态。
fn tool_display_state_for_record_kind(record_kind: SessionMessageRecordKind) -> &'static str {
    match record_kind {
        SessionMessageRecordKind::ToolResultOutput => "completed",
        _ => "running",
    }
}

/// Injects one display-only execution state flag into tool-card metadata without affecting the model-facing protocol payload.
/// 向工具卡片元数据注入一个仅用于显示的执行状态标记，而不影响面向模型的协议载荷。
fn attach_tool_display_state_metadata(metadata: Option<Value>, state: &str) -> Option<Value> {
    let mut metadata_object = match metadata {
        Some(Value::Object(object)) => object,
        _ => serde_json::Map::new(),
    };
    metadata_object.insert(
        String::from("tool_display_state"),
        Value::String(state.to_string()),
    );
    Some(Value::Object(metadata_object))
}

/// Parses one UUID-like session identifier text into the local `SessionId` type when the input is non-empty and valid.
/// 当输入非空且格式合法时，将一段类 UUID 的会话标识文本解析为本地 `SessionId` 类型。
fn parse_session_id_text(value: Option<&str>) -> Option<SessionId> {
    let value = value?.trim();
    (!value.is_empty())
        .then_some(value)
        .and_then(|value| SessionId::parse_str(value).ok())
}

/// Parses one persisted task-session payload object into a cacheable subagent task panel state.
/// 将一份持久化 task 会话载荷对象解析为可缓存的子代理任务面板状态。
fn parse_subagent_task_panel_payload(
    parent_session_id: SessionId,
    payload: &Value,
) -> Option<SubagentTaskPanelState> {
    let task_id = task_session_internal_id_from_payload(payload)?;
    let run_state = match payload
        .get("execution_status")
        .and_then(Value::as_str)
        .unwrap_or("completed")
    {
        "running" => SubagentTaskRunState::Running,
        "stopped" => SubagentTaskRunState::Stopped,
        "failed" => SubagentTaskRunState::Failed,
        _ => SubagentTaskRunState::Completed,
    };
    let description = payload
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty());
    let public_task_id = task_session_public_id_from_payload(payload)
        .map(str::to_string)
        .unwrap_or_else(|| task_id.to_string());
    let subagent_label = task_session_tool_display_label(
        task_session_display_title_from_payload(payload),
        description.as_deref(),
        payload.get("subagent").and_then(Value::as_str),
        Some(public_task_id.as_str()),
        payload.get("prompt").and_then(Value::as_str),
    );

    Some(SubagentTaskPanelState {
        task_id,
        public_task_id,
        parent_session_id,
        subagent_label,
        description,
        run_state,
        result_text: payload
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default(),
        conversation_state: payload
            .get("conversation_state")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
    })
}

/// Parses one merged task-session metadata object into a cacheable subagent task panel state.
/// 将一份合并后的 task 会话元数据对象解析为可缓存的子代理任务面板状态。
fn parse_subagent_task_panel_metadata(
    parent_session_id: SessionId,
    metadata: &Value,
) -> Option<SubagentTaskPanelState> {
    (metadata.get("kind").and_then(Value::as_str) == Some("task_session")).then_some(())?;
    parse_subagent_task_panel_payload(parent_session_id, metadata.get("payload")?)
}

/// Extracts the task prompt text from one merged task-session metadata object when the host payload keeps it.
/// 当宿主载荷保留了任务提示词时，从一份合并后的 task 会话元数据对象中提取该提示词文本。
fn parse_subagent_task_prompt_metadata(metadata: &Value) -> Option<String> {
    (metadata.get("kind").and_then(Value::as_str) == Some("task_session")).then_some(())?;
    metadata
        .get("payload")?
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

/// Returns whether one clipboard text payload should enable the composer paste action.
/// 返回某段剪贴板文本载荷是否应启用提示词粘贴动作。
fn composer_clipboard_text_is_pastable(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.is_empty())
}

/// One undo snapshot retained for the composer editor.
/// 为提示词编辑器保留的一条撤销快照。
#[derive(Debug, Clone)]
struct ComposerUndoSnapshot {
    /// Full text payload captured before one mutating composer action.
    /// 在某次会修改提示词内容的动作之前捕获的完整文本载荷。
    text: String,
    /// Attachment list captured before one mutating composer action.
    /// 在某次会修改提示词内容的动作之前捕获的附件列表。
    attachments: Vec<ConversationAttachment>,
    /// Cursor and selection position captured before one mutating composer action.
    /// 在某次会修改提示词内容的动作之前捕获的光标与选区位置。
    cursor: text_editor::Cursor,
    /// Internal scroll offset mirrored from the composer editor at capture time.
    /// 捕获快照时从提示词编辑器镜像得到的内部滚动偏移量。
    scroll_offset: f32,
    /// Optional attachment lease scope retained by this undo snapshot.
    /// 该撤销快照持有的可选附件租约作用域。
    attachment_ref_scope: Option<String>,
    /// Optional workspace path that owns the retained attachment lease scope.
    /// 持有所保留附件租约作用域的可选工作区路径。
    attachment_workspace_path: Option<String>,
}

/// Confirm action carried by the reusable blocking notice modal.
/// 可复用阻塞提示弹窗携带的确认动作。
#[derive(Debug, Clone)]
enum WorkbenchNoticeConfirmAction {
    /// Closes the modal without mutating domain state.
    /// 关闭弹窗且不修改业务状态。
    Acknowledge,
    /// Applies one pending model switch and removes staged attachments unsupported by the target model.
    /// 应用一次待确认模型切换，并移除目标模型不支持的已暂存附件。
    ApplyModelSwitchRemovingUnsupportedAttachments {
        /// Pending model choice selected by the user before compatibility confirmation.
        /// 用户在兼容性确认之前选择的待切换模型。
        choice: WorkbenchModelChoice,
    },
    /// Removes one configured model provider after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条模型供应商配置。
    RemoveConfiguredProvider {
        /// Stable configured provider identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定模型供应商标识。
        provider_id: String,
    },
    /// Removes one shared capability vendor after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条共享能力供应商配置。
    RemoveAiCapabilityVendor {
        /// Stable shared capability vendor identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定共享能力供应商标识。
        vendor_id: String,
    },
    /// Removes one configured MCP server after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条 MCP 服务器配置。
    RemoveMcpServerConfiguration {
        /// Stable configured MCP server identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定 MCP 服务器标识。
        server_id: String,
    },
}

/// Reusable blocking notice modal state shared by acknowledgement and confirmation prompts.
/// 告知与确认提示共享的可复用阻塞提示弹窗状态。
#[derive(Debug, Clone)]
struct WorkbenchNoticeModalState {
    /// Modal title shown in the header row.
    /// 显示在弹窗标题行中的标题。
    title: String,
    /// Modal body explaining the condition that needs acknowledgement.
    /// 解释需要确认条件的弹窗正文。
    body: String,
    /// Primary confirmation button label.
    /// 主确认按钮标签。
    confirm_label: String,
    /// Optional secondary dismiss button label.
    /// 可选次要取消按钮标签。
    dismiss_label: Option<String>,
    /// Domain action executed when the primary confirmation button is pressed.
    /// 点击主确认按钮时执行的业务动作。
    confirm_action: WorkbenchNoticeConfirmAction,
}

/// Blocking image preview modal state opened from one staged image attachment tile.
/// 从已暂存图片附件方格打开的阻塞式图片预览弹窗状态。
#[derive(Debug, Clone)]
struct WorkbenchImagePreviewModalState {
    /// File name shown in the preview modal header.
    /// 显示在预览弹窗标题栏中的文件名。
    file_name: String,
    /// Local image path rendered by the preview surface.
    /// 由预览区域渲染的本地图片路径。
    local_path: String,
    /// Current zoom factor applied to the preview image surface.
    /// 当前应用到图片预览区域的缩放倍率。
    zoom: f32,
    /// Last window cursor position captured while left-button panning is active.
    /// 左键平移激活时捕获的上一帧窗口光标位置。
    pan_pointer: Option<iced::Point>,
    /// Latest local cursor position tracked inside the preview image surface.
    /// 最近一次在图片预览区域内部追踪到的局部光标位置。
    preview_pointer: Option<iced::Point>,
    /// Latest absolute scroll offset reported by the preview image scrollable.
    /// 图片预览滚动容器最近一次回传的绝对滚动偏移。
    scroll_offset: scrollable::AbsoluteOffset,
}

/// Host-side generated-audio playback state retained while one inline audio strip is actively playing.
/// 当某条内联音频条正在播放时保留的宿主侧生成音频播放状态。
struct GeneratedAudioPlaybackState {
    /// Local absolute path of the currently playing audio file.
    /// 当前正在播放的音频文件本地绝对路径。
    local_path: String,
    /// Output stream kept alive for the lifetime of the active sink.
    /// 为当前激活播放 sink 保活的输出流。
    _stream: OutputStream,
    /// Active sink responsible for decoding and playing the generated audio file.
    /// 负责解码并播放生成音频文件的激活 sink。
    sink: Sink,
}

/// Live scrollbar geometry derived from the current composer editor content and viewport.
/// 根据当前提示词编辑器内容与视口推导出的实时滚动条几何信息。
#[derive(Debug, Clone, Copy)]
struct ComposerScrollbarLayout {
    /// Height of the scrollbar track after applying vertical paddings.
    /// 应用上下留白后的滚动条轨道高度。
    track_height: f32,
    /// Height of the draggable scrollbar thumb.
    /// 可拖拽滚动条滑块高度。
    thumb_height: f32,
    /// Maximal distance the scrollbar thumb can travel inside the track.
    /// 滚动条滑块在轨道内部可移动的最大距离。
    thumb_travel: f32,
    /// Current thumb top offset inside the track.
    /// 当前滑块在轨道内部的顶部偏移量。
    thumb_top: f32,
    /// Maximal vertical scroll offset allowed by the current content.
    /// 当前内容允许的最大纵向滚动偏移量。
    max_scroll_offset: f32,
}

/// One action row rendered by the shared workbench context menu.
/// 由共享工作台右键菜单渲染的一行动作。
#[derive(Debug, Clone)]
struct WorkbenchContextMenuItem {
    /// User-facing label displayed inside the menu row.
    /// 菜单行内部显示的面向用户标签。
    label: String,
    /// Message emitted when the menu row is enabled and pressed.
    /// 菜单行启用并被点击时发出的消息。
    message: Option<Message>,
}

/// Identifies the feature surface that opened the shared workbench context menu.
/// 标识打开共享工作台右键菜单的功能表面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkbenchContextMenuSource {
    /// Menu opened from the prompt composer editor.
    /// 从提示词编辑器打开的菜单。
    Composer,
    /// Menu opened from the left task tree.
    /// 从左侧任务树打开的菜单。
    TaskTree,
    /// Menu opened from the rendered transcript surface.
    /// 从渲染转录表面打开的菜单。
    Transcript,
}

/// Runtime state for one open shared workbench context menu.
/// 单个已打开共享工作台右键菜单的运行时状态。
#[derive(Debug, Clone)]
struct WorkbenchContextMenuState {
    /// Feature surface that opened this shared menu.
    /// 打开此共享菜单的功能表面。
    source: WorkbenchContextMenuSource,
    /// Window-relative position used to anchor the menu before clamping.
    /// 进行边界钳制前用于锚定菜单的窗口相对位置。
    anchor: iced::Point,
    /// Fixed width used to render the menu body.
    /// 渲染菜单主体时使用的固定宽度。
    width: f32,
    /// Ordered action rows shown inside the menu.
    /// 菜单内部显示的有序动作行。
    items: Vec<WorkbenchContextMenuItem>,
}

impl WorkbenchContextMenuItem {
    /// Builds one enabled shared context-menu item.
    /// 构建一个启用状态的共享右键菜单项。
    fn enabled(label: impl Into<String>, message: Message) -> Self {
        Self {
            label: label.into(),
            message: Some(message),
        }
    }

    /// Builds one disabled shared context-menu item.
    /// 构建一个禁用状态的共享右键菜单项。
    fn disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            message: None,
        }
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
    .font(iced_aw::ICED_AW_FONT_BYTES)
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
    /// Latest keyboard modifier state used by global mouse gestures such as Ctrl + wheel.
    /// 全局鼠标手势（例如 Ctrl + 滚轮）使用的最新键盘修饰键状态。
    workbench_keyboard_modifiers: keyboard::Modifiers,
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
    /// Structured multimodal attachments currently staged beside the prompt composer text.
    /// 当前与提示词文本一起暂存的结构化多模态附件集合。
    composer_attachments: Vec<ConversationAttachment>,
    /// Reusable blocking notice modal currently shown above all main-workbench content.
    /// 当前显示在所有主工作台内容之上的可复用阻塞提示弹窗。
    workbench_notice_modal: Option<WorkbenchNoticeModalState>,
    /// Blocking image preview modal currently opened from one staged composer attachment.
    /// 当前从某个已暂存提示词附件打开的阻塞式图片预览弹窗。
    workbench_image_preview_modal: Option<WorkbenchImagePreviewModalState>,
    /// Host-side generated-audio playback state used by transcript media strips and tool-modal preview cards.
    /// 供转录媒体条与工具弹层预览卡片使用的宿主侧生成音频播放状态。
    generated_audio_playback: Option<GeneratedAudioPlaybackState>,
    /// Human-readable file labels currently hovering over the main workbench window as potential composer drops.
    /// 当前悬停在主工作台窗口上、可作为提示词拖拽导入候选项的人类可读文件标签集合。
    composer_drag_hover_labels: Vec<String>,
    /// Undo snapshots retained for the main prompt composer.
    /// 为主提示词编辑器保留的撤销快照集合。
    composer_undo_history: Vec<ComposerUndoSnapshot>,
    /// Current internal scroll offset mirrored from the capped prompt composer editor.
    /// 从封顶提示词编辑器内部镜像得到的当前滚动偏移量。
    composer_scroll_offset: f32,
    /// Latest pointer y position tracked inside the composer editor surface for scrollbar interactions.
    /// 为滚动条交互跟踪的提示词编辑器表面内部最近一次指针纵向位置。
    composer_scrollbar_pointer_y: Option<f32>,
    /// Current grabbed offset inside the draggable composer scrollbar thumb, when dragging is active.
    /// 当前处于激活拖拽状态时在提示词滚动条滑块内部抓取到的偏移量。
    composer_scrollbar_drag_grab_offset: Option<f32>,
    /// Currently open shared workbench context menu, if any.
    /// 当前已打开的共享工作台右键菜单（如果存在）。
    workbench_context_menu: Option<WorkbenchContextMenuState>,
    /// Latest clipboard text snapshot loaded for composer menu availability and paste actions.
    /// 为提示词菜单可用性与粘贴动作载入的最新剪贴板文本快照。
    composer_context_clipboard_text: Option<String>,
    /// Last host-driven selectable transcript command emitted from a shared context menu.
    /// 最近一次由共享右键菜单发出的宿主侧可选择转录命令。
    selectable_transcript_command: Option<SelectableTranscriptCommand>,
    /// Monotonic revision assigned to selectable transcript commands.
    /// 分配给可选择转录命令的单调修订号。
    selectable_transcript_command_revision: u64,
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
    /// Cached hunk payloads reused by virtualized diff rendering.
    /// 供虚拟化 Diff 渲染复用的已缓存 hunk 载荷。
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
    /// Current drop target preview shown while one pending-prompt queue row is being reordered.
    /// 当前在待发送队列项重排期间展示的插入目标预览。
    active_pending_prompt_drop_target: Option<PendingPromptDropTarget>,
    /// Pending click-or-drag interaction captured from one pending-prompt queue handle press.
    /// 从某次待发送队列拖拽柄按下动作中捕获的待判定点击或拖拽交互状态。
    pending_pending_prompt_drag: Option<PendingPendingPromptDrag>,
    /// Queue entry identifier currently being reordered through drag interaction.
    /// 当前正在通过拖拽交互调整顺序的待发送队列项标识。
    dragging_pending_prompt_entry_id: Option<String>,
    /// Current drop target preview shown while one enabled AI capability target is being reordered.
    /// 当前在已启用 AI 能力目标重排期间展示的插入目标预览。
    active_ai_capability_route_drop_target: Option<AiCapabilityRouteDropTarget>,
    /// Pending click-or-drag interaction captured from one AI capability target drag handle press.
    /// 从某次 AI 能力目标拖拽柄按下动作中捕获的待判定点击或拖拽交互状态。
    pending_ai_capability_route_drag: Option<PendingAiCapabilityRouteDrag>,
    /// Fully qualified capability target selector currently being reordered through drag interaction.
    /// 当前正在通过拖拽交互调整顺序的完整能力目标选择器。
    dragging_ai_capability_route_target: Option<String>,
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
    /// Session-scoped full tool-detail transcript rows kept outside the main render projection.
    /// 独立于主渲染投影保存的按会话归属完整工具明细转录行。
    session_tool_detail_messages: BTreeMap<SessionId, BTreeMap<String, SessionMessageEntry>>,
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
    /// Session identifiers whose transcript and runtime payloads have been lazily hydrated.
    /// 已经完成转录与运行时载荷懒加载的会话标识集合。
    hydrated_session_ids: BTreeSet<SessionId>,
    /// Session identifiers whose transcript/runtime payloads are safe to replace on the next save.
    /// 下次保存时可以安全替换转录与运行时载荷的会话标识集合。
    dirty_session_payload_ids: BTreeSet<SessionId>,
    /// Session identifiers whose transcript rows changed and may be replaced on the next save.
    /// 转录行已经发生变化、下次保存时可以替换的会话标识集合。
    dirty_session_message_ids: BTreeSet<SessionId>,
    /// Session identifiers currently running the first lazy hydration task.
    /// 当前正在执行首次懒加载任务的会话标识集合。
    loading_session_ids: BTreeSet<SessionId>,
    /// Session identifiers currently materializing the heavy unified agent-view projections on a background worker thread.
    /// 当前正在后台工作线程中物化较重统一 agent-view 投影的会话标识集合。
    loading_session_agent_view_projection_ids: BTreeSet<SessionId>,
    /// Earliest loaded transcript ordinal for each hydrated session.
    /// 每个已水合会话当前已加载的最早转录序号。
    session_history_earliest_ordinals: BTreeMap<SessionId, i64>,
    /// Session identifiers that still have older transcript rows available.
    /// 仍然存在更早转录行可继续加载的会话标识集合。
    session_history_has_more: BTreeSet<SessionId>,
    /// Session identifiers currently loading one older transcript page.
    /// 当前正在加载更早转录分页的会话标识集合。
    loading_older_session_history_ids: BTreeSet<SessionId>,
    /// Aggregated left-panel workspace-history search UI state shared across projects and sessions.
    /// 跨项目与会话共享的左侧面板工作台历史搜索聚合界面状态。
    workspace_history_search: WorkspaceHistorySearchPanelState,
    /// Low-priority legacy maintenance queue that scans sessions without hydrating transcripts.
    /// 不水合转录正文的低优先级旧数据维护会话队列。
    background_legacy_maintenance_queue: VecDeque<SessionId>,
    /// Session identifier currently being processed by the background legacy maintenance queue.
    /// 当前正在被后台旧数据维护队列处理的会话标识。
    background_legacy_maintenance_in_flight: Option<SessionId>,
    /// Number of sessions completed by the current background legacy maintenance queue.
    /// 当前后台旧数据维护队列已完成的会话数量。
    background_legacy_maintenance_completed_count: usize,
    /// Number of sessions that failed in the current background legacy maintenance queue.
    /// 当前后台旧数据维护队列中维护失败的会话数量。
    background_legacy_maintenance_failed_count: usize,
    /// Whether one workspace snapshot persistence task is currently writing to SQLite.
    /// 当前是否已有一条工作台快照持久化任务正在写入 SQLite。
    workspace_snapshot_persist_in_flight: bool,
    /// Whether another workspace snapshot save should be replayed once the in-flight persistence finishes.
    /// 当前飞行中的持久化完成后是否还需要补跑一次新的工作台快照保存。
    workspace_snapshot_persist_pending: bool,
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
    /// Session-scoped cached subagent task panels keyed first by parent session and then by stable child task identifier.
    /// 先按父会话、再按稳定子任务标识索引的按会话归属子代理任务面板缓存集合。
    session_subagent_tasks: BTreeMap<SessionId, BTreeMap<SessionId, SubagentTaskPanelState>>,
    /// Parent sessions whose current execution round should keep the subagent navigation panel visible until the round fully settles.
    /// 当前执行轮次应保持子代理导航面板可见、直到整轮完全收敛的父会话集合。
    session_subagent_navigation_rounds: BTreeSet<SessionId>,
    /// Currently opened subagent panel that temporarily replaces the main parent transcript surface.
    /// 当前已打开、并临时替代主父会话转录表面的子代理面板选择状态。
    active_subagent_panel: Option<SelectedSubagentTaskPanel>,
    /// Whether the left-edge subagent navigation panel is currently expanded by pointer hover.
    /// 左侧贴边子代理导航面板当前是否因鼠标悬停而展开。
    subagent_navigation_expanded: bool,
    /// Detached subagent task identifiers that already own one live background listener task in the GUI layer.
    /// 已经在图形界面层挂接后台实时监听任务的分离子代理任务标识集合。
    detached_subagent_task_listeners: BTreeSet<SessionId>,
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
    /// Prepared inline transcript snapshots keyed by visible session id and reused by the main rendered surface so normal GUI view passes do not deep-clone the full structural transcript tree.
    /// 以可见会话标识为键保存的已准备内联转录快照，供主渲染表面复用，避免普通界面刷新时深拷贝整份结构化转录树。
    session_inline_agent_view_documents: BTreeMap<SessionId, SessionInlineAgentViewDocumentCache>,
    /// Unified block collapsed states keyed by session id and stable block identifier.
    /// 以会话标识与稳定区块标识为键保存的统一块折叠状态。
    session_agent_view_block_collapsed: BTreeMap<SessionId, BTreeMap<String, bool>>,
    /// Session-scoped flag telling whether the transcript should keep auto-following the newest tail output.
    /// 以会话为粒度保存的转录自动追随最新尾部输出标记。
    session_agent_view_follow_tail: BTreeMap<SessionId, bool>,
    /// Latest transcript scrollable viewport keyed by session id.
    /// 以会话标识为键保存的最新转录滚动视口。
    session_agent_view_viewports: BTreeMap<SessionId, scrollable::Viewport>,
    /// Pending transcript tail-follow requests coalesced while streaming output is arriving.
    /// 流式输出到达期间被合并处理的待执行转录尾部追随请求。
    pending_transcript_follow_tail_session_ids: BTreeSet<SessionId>,
    /// Active drag auto-scroll state for the main rendered transcript surface.
    /// 主渲染转录表面的当前拖拽自动滚动状态。
    active_transcript_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Latest floating thinking-overlay viewport keyed by session id.
    /// 以会话标识为键保存的最新浮动思考层视口。
    session_streaming_thinking_viewports: BTreeMap<SessionId, scrollable::Viewport>,
    /// Pending floating thinking-overlay tail-follow requests coalesced while streaming thinking is arriving.
    /// 流式思考到达期间被合并处理的待执行浮层尾部追随请求。
    pending_thinking_overlay_follow_tail_session_ids: BTreeSet<SessionId>,
    /// Active drag auto-scroll state for the floating thinking overlay body.
    /// 流式思考浮层正文的当前拖拽自动滚动状态。
    active_thinking_overlay_drag_auto_scroll: Option<ActiveTranscriptDragAutoScroll>,
    /// Surface mode currently used by the unified transcript panel.
    /// 当前统一转录面板使用的表面模式。
    agent_transcript_surface_mode: AgentTranscriptSurfaceMode,
    /// Session-scoped execution phases reflected by the workbench status bar.
    /// 反映在工作台状态条中的按会话归属执行阶段集合。
    session_run_phases: BTreeMap<SessionId, SessionRunPhase>,
    /// Per-session active real-AI run state keyed by session identifier.
    /// 以会话标识为键保存的按会话划分活动真实 AI 运行状态。
    active_runs: BTreeMap<SessionId, ActiveSessionRunState>,
    /// Monotonic identifier seed used to distinguish restarted or stopped execution streams.
    /// 用于区分重启后或已停止执行流的单调递增标识种子。
    next_run_id: u64,
    /// Current startup lifecycle shown by the full-screen loading overlay.
    /// 由全屏载入遮罩展示的当前启动生命周期状态。
    startup_lifecycle: StartupLifecycle,
    /// Optional controller runtime handle retained after the shared data plane boots.
    /// 在共享数据平面完成引导后保留的可选控制器运行时句柄。
    controller_runtime: Option<ControllerRuntimeHandle>,
    /// Optional LuaSkills runtime handle retained after the secondary startup phase completes.
    /// 在二级启动阶段完成后保留的可选 LuaSkills 运行时句柄。
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Optional stable MCP runtime access surface retained after the background MCP bootstrap completes.
    /// 在后台 MCP 引导完成后保留的可选稳定 MCP 运行时访问面。
    mcp_runtime: Option<McpRuntimeAccess>,
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
    /// Standalone AI capability tool configuration loaded from the dedicated Vulcan AI capability file.
    /// 从独立 Vulcan AI 能力配置文件载入的独立 AI 能力工具配置。
    ai_tool_configuration: PersistedAiToolConfiguration,
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
    /// Currently selected section inside the dedicated settings workspace.
    /// 当前在独立设置工作区中选中的分区。
    selected_settings_section: SettingsSection,
    /// Whether the left settings tree should keep the AI capability subtree expanded.
    /// 左侧设置树是否保持 AI 能力子树展开。
    settings_ai_capability_tree_expanded: bool,
    /// Whether the left settings tree should keep the model-settings subtree expanded.
    /// 左侧设置树是否保持模型设置子树展开。
    settings_model_tree_expanded: bool,
    /// Optional blocking vendor picker modal used by the supplier page before one new shared AI capability vendor is created.
    /// 供应商页在创建新的共享 AI 能力供应商前使用的可选阻塞式供应商选择弹窗。
    ai_capability_vendor_picker: Option<AiCapabilityVendorPickerState>,
    /// Optional blocking vendor editor modal used when one shared AI capability vendor is added or edited.
    /// 新增或编辑共享 AI 能力供应商时使用的可选阻塞式供应商编辑弹窗。
    ai_capability_vendor_editor: Option<AiCapabilityVendorEditorState>,
    /// Optional blocking picker modal used by capability pages to batch-select vendor targets before saving them into one capability route.
    /// 供能力页在保存前批量选择供应商目标使用的可选阻塞式选择器弹窗。
    ai_capability_target_picker: Option<AiCapabilityTargetPickerState>,
    /// Optional blocking picker modal used by capability pages to choose one routing strategy from a compact selector.
    /// 供能力页从紧凑选择器中挑选单条路由策略时使用的可选阻塞式弹窗。
    ai_capability_routing_picker: Option<AiCapabilityRoutingPickerState>,
    /// Optional blocking editor modal used when one configured model provider is created or updated.
    /// 新增或更新模型供应商配置时使用的可选阻塞式编辑弹窗。
    provider_editor: Option<ProviderEditorState>,
    /// Optional blocking model-picker modal used to edit one provider's enabled-model draft before explicit save.
    /// 在显式保存前编辑某个供应商启用模型草稿时使用的可选阻塞式模型配置弹窗。
    provider_model_picker: Option<ProviderModelPickerState>,
    /// Optional blocking per-model metadata editor used to correct one enabled model's capability declaration.
    /// 用于修正单个已启用模型能力声明的可选阻塞式逐模型元数据编辑弹窗。
    provider_model_override_editor: Option<ProviderModelOverrideEditorState>,
    /// Currently selected provider entry inside the configuration page.
    /// 当前在配置页面中选中的供应商条目。
    selected_provider: ProviderChoice,
    /// Editable provider form state for the selected provider.
    /// 当前选中供应商对应的可编辑表单状态。
    provider_form: ProviderFormState,
    /// Optional provider-configuration feedback message shown near the configuration widgets.
    /// 展示在配置控件附近的供应商配置反馈消息。
    provider_notice: String,
    /// Optional standalone AI capability configuration feedback message shown near the AI-tool settings widgets.
    /// 展示在独立 AI 能力设置控件附近的反馈消息。
    ai_tool_notice: String,
    /// Optional MCP-configuration feedback message shown near the MCP settings widgets.
    /// 展示在 MCP 设置控件附近的 MCP 配置反馈消息。
    mcp_notice: String,
    /// Currently selected MCP server identifier inside the settings page.
    /// 当前在设置页面中选中的 MCP 服务器标识。
    selected_mcp_server_id: Option<String>,
    /// Whether the standalone MCP server editor overlay is currently visible.
    /// 当前是否显示独立 MCP 服务器编辑遮罩层。
    mcp_server_editor_open: bool,
    /// Editable MCP server form state for the selected MCP server entry.
    /// 当前选中 MCP 服务器条目对应的可编辑表单状态。
    mcp_server_form: McpServerFormState,
    /// Human-readable OAuth status summary for the currently selected MCP server.
    /// 当前选中 MCP 服务器对应的人类可读 OAuth 状态摘要。
    mcp_oauth_status: String,
    /// Raw manual callback input buffer used when the browser cannot return to the local listener automatically.
    /// 当浏览器无法自动返回本地监听器时使用的原始手动回调输入缓冲区。
    mcp_oauth_callback_input: String,
    /// Latest authorization URL generated for the currently selected MCP server.
    /// 为当前选中 MCP 服务器生成的最近一次授权 URL。
    mcp_oauth_authorization_url: String,
    /// Pending OAuth state retained while the host waits for a browser callback or manual paste.
    /// 当宿主等待浏览器回调或手动粘贴时保留的待完成 OAuth state。
    mcp_oauth_pending_state: Option<String>,
    /// Whether one MCP OAuth flow is currently running.
    /// 当前是否存在一条正在进行中的 MCP OAuth 流程。
    mcp_oauth_flow_in_flight: bool,
    /// Project identifier currently owning the project-settings modal, when the modal is open.
    /// 当前拥有项目设置弹窗的项目标识（当弹窗打开时存在）。
    project_settings_project_id: Option<String>,
    /// Date-picker cutoff date used by the project-scoped historical attachment cleanup tool.
    /// 项目级历史附件清理工具使用的日期选择器截止日期。
    project_attachment_cleanup_cutoff_date: Date,
    /// Whether the project attachment cleanup date picker overlay is open.
    /// 项目附件清理日期选择器浮层当前是否打开。
    project_attachment_cleanup_date_picker_open: bool,
    /// Attachment-kind selection currently used by the project-scoped cleanup tool.
    /// 当前供项目级清理工具使用的附件类型选择状态。
    project_attachment_cleanup_selection: AttachmentCleanupSelection,
    /// Latest project-scoped cleanup preview built from the selected cutoff date and kind filters.
    /// 根据当前截止日期与类型筛选构建出的最近一次项目级清理预览。
    project_attachment_cleanup_preview: Option<AttachmentCleanupPreview>,
    /// Latest successful project-scoped cleanup execution report shown inside the project settings modal.
    /// 展示在项目设置弹窗中的最近一次成功项目级清理执行报告。
    project_attachment_cleanup_last_report: Option<AttachmentCleanupReport>,
    /// Modal-scoped notice rendered beside the project-scoped cleanup controls.
    /// 展示在项目级清理控件旁边的弹窗局部提示信息。
    project_attachment_cleanup_notice: String,
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
    /// Remembered workbench reasoning preference chosen by the user before model-capability reconciliation is applied.
    /// 在应用模型能力回退前由用户选定并记忆的工作台思考偏好。
    workbench_reasoning_effort_preference: WorkbenchReasoningEffort,
    /// Effective workbench reasoning effort currently supported by the active model and sent to execution requests.
    /// 当前被激活模型真实支持并会写入执行请求的工作台生效思考档位。
    workbench_reasoning_effort: WorkbenchReasoningEffort,
    /// Workbench-scoped web-search override mode applied to newly queued prompts and direct sends.
    /// 应用于新建排队提示词与直接发送的工作台网页搜索覆盖模式。
    workbench_web_search_override: PendingPromptWebSearchOverride,
    /// Optional third-party provider identifier used when one specific web-search entry is selected in the workbench.
    /// 当工作台选中单个指定网页搜索条目时使用的可选第三方供应商标识。
    workbench_web_search_provider_id: Option<String>,
    /// Semantic AI capability kinds temporarily disabled for the current workbench send chain without mutating persisted target preferences.
    /// 在不修改持久化目标偏好的前提下，为当前工作台发送链路临时关闭的语义 AI 能力类型集合。
    workbench_disabled_ai_capability_kinds: BTreeSet<AiToolKind>,
    /// Whether the quick AI capability panel opened from the composer controls is currently visible.
    /// 从提示词控制区打开的快捷 AI 能力面板当前是否可见。
    workbench_ai_capability_panel_open: bool,
    /// Currently selected section inside the workbench quick AI capability panel.
    /// 当前在工作台快捷 AI 能力面板中选中的分区。
    workbench_ai_capability_section: WorkbenchAiCapabilitySection,
    /// Whether the in-app workbench model-picker modal is currently visible.
    /// 工作台内嵌模型选择弹窗当前是否可见。
    workbench_model_picker_open: bool,
    /// Whether the compact in-app reasoning-effort picker modal is currently visible.
    /// 工作台内嵌思考级别选择弹窗当前是否可见。
    workbench_reasoning_picker_open: bool,
    /// Current free-text filter value applied inside the workbench model-picker modal.
    /// 当前在工作台模型选择弹窗内部应用的自由文本过滤值。
    workbench_model_picker_filter: String,
    /// Currently highlighted filtered option index inside the workbench model-picker modal.
    /// 当前在工作台模型选择弹窗中过滤结果内高亮的选项索引。
    workbench_model_picker_highlighted_index: Option<usize>,
    /// Current vertical scroll offset tracked by the virtualized workbench model-picker list.
    /// 当前由虚拟化工作台模型选择列表跟踪的纵向滚动偏移量。
    workbench_model_picker_scroll_offset: f32,
    /// Current viewport height reported by the virtualized workbench model-picker list.
    /// 当前由虚拟化工作台模型选择列表上报的可视高度。
    workbench_model_picker_viewport_height: f32,
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
    /// Human-readable context usage detail shown beside the budget bar.
    /// 展示在预算条旁边的人类可读上下文占用详情。
    context_usage_detail: String,
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

/// Pending drag candidate captured from one pending-prompt queue handle press.
/// 从某次待发送队列拖拽柄按下动作中捕获的待确认拖拽候选状态。
#[derive(Debug, Clone)]
struct PendingPendingPromptDrag {
    /// Stable root-session identifier that owns the queue being reordered.
    /// 拥有当前重排队列的稳定根会话标识。
    session_id: SessionId,
    /// Stable queue-entry identifier that may turn into the active drag source.
    /// 可能升级为激活拖拽源的稳定队列项标识。
    entry_id: String,
    /// Cursor origin captured when the drag candidate was pressed.
    /// 按下拖拽候选时捕获到的鼠标起始位置。
    origin: iced::Point,
}

/// Pending drag candidate captured from one AI capability route handle press.
/// 从某次 AI 能力路由拖拽柄按下动作中捕获的待确认拖拽候选状态。
#[derive(Debug, Clone)]
struct PendingAiCapabilityRouteDrag {
    /// Semantic capability owning the route currently being reordered.
    /// 当前正在重排路由所属的语义能力。
    capability_kind: AiToolKind,
    /// Fully qualified `vendor/target` selector that may become the active drag source.
    /// 可能升级为激活拖拽源的完整 `供应商/目标` 选择器。
    target_selector: String,
    /// Cursor origin captured when the drag candidate was pressed.
    /// 按下拖拽候选时捕获到的鼠标起始位置。
    origin: iced::Point,
}

/// Stable scope selection used by the left-panel workspace-history search.
/// 左侧面板工作台历史搜索使用的稳定范围选择。
#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkspaceHistorySearchScope {
    /// Searches across every visible project and session.
    /// 跨全部可见项目与会话执行搜索。
    Global,
    /// Restricts search to one concrete visible project identifier.
    /// 将搜索限制到某个具体的可见项目标识。
    Project(String),
}

impl Default for WorkspaceHistorySearchScope {
    /// Returns the default global scope used before one session-derived project scope is chosen.
    /// 返回在根据当前会话推导项目范围前使用的默认全局范围。
    fn default() -> Self {
        Self::Global
    }
}

/// One selectable scope option rendered inside the left-panel workspace-history project dropdown.
/// 渲染在左侧工作台历史项目下拉列表中的单条可选范围项。
#[derive(Debug, Clone)]
struct WorkspaceHistorySearchScopeOption {
    /// Stable scope value applied when this option is selected.
    /// 选中该项后会应用的稳定范围值。
    scope: WorkspaceHistorySearchScope,
    /// Human-readable option label shown to the user.
    /// 展示给用户的人类可读选项标签。
    label: String,
    /// Searchable helper text used by the project keyword filter.
    /// 供项目关键字过滤使用的可搜索辅助文本。
    filter_text: String,
}

/// Pending transcript navigation target retained while one search result is opening or hydrating.
/// 当某条搜索结果正在打开或补载时保留的待定位转录目标。
#[derive(Debug, Clone)]
struct PendingWorkspaceHistorySearchNavigation {
    /// Stable session identifier owning the matched message row.
    /// 拥有该命中消息行的稳定会话标识。
    session_id: SessionId,
    /// Stable message identifier that should become visible inside the transcript.
    /// 应在转录中可见的稳定消息标识。
    message_id: String,
    /// Stable ordinal used to decide whether older history still needs loading.
    /// 用于判断是否仍需补载更早历史的稳定序号。
    ordinal: i64,
    /// Stable turn index used when reporting search-navigation context to the user.
    /// 在向用户回报搜索定位上下文时使用的稳定轮次索引。
    turn_index: i64,
}

/// Active transcript highlight target retained after one workspace-history search hit has been opened successfully.
/// 某条工作台历史搜索命中成功打开后保留的活动转录高亮目标。
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceHistorySearchHighlightTarget {
    /// Stable session identifier owning the highlighted visible message row.
    /// 拥有当前高亮可见消息行的稳定会话标识。
    session_id: SessionId,
    /// Stable visible message identifier that should stay highlighted inside the transcript.
    /// 应在转录中保持高亮的稳定可见消息标识。
    message_id: String,
}

/// Aggregated state retained by the left-panel workspace-history search UI.
/// 左侧面板工作台历史搜索界面保留的聚合状态。
#[derive(Debug, Clone, Default)]
struct WorkspaceHistorySearchPanelState {
    /// Whether the dedicated search panel is currently expanded.
    /// 当前专用搜索面板是否处于展开状态。
    panel_open: bool,
    /// Current query text shown inside the search input.
    /// 显示在搜索输入框中的当前查询文本。
    query: String,
    /// Latest search result page returned by the BM25 workspace query.
    /// BM25 工作区查询返回的最近一页搜索结果。
    results: Option<WorkspaceHistorySearchPage>,
    /// Whether one workspace-history search request is currently in flight.
    /// 当前是否有一条工作台历史搜索请求正在执行。
    loading: bool,
    /// Whether the current in-flight search request is loading an additional result page.
    /// 当前正在执行中的搜索请求是否属于继续加载下一页结果。
    loading_more: bool,
    /// Latest search error shown inline to the user, if any.
    /// 若存在，则为当前内联展示给用户的最近一次搜索错误。
    error: Option<String>,
    /// Current visible search scope.
    /// 当前可见搜索范围。
    scope: WorkspaceHistorySearchScope,
    /// Whether the project selector dropdown is currently expanded.
    /// 当前项目选择下拉区是否处于展开状态。
    project_picker_open: bool,
    /// Current project keyword filter text.
    /// 当前项目关键字过滤文本。
    project_filter: String,
    /// Optional pending transcript navigation target created from one clicked search result.
    /// 由某条已点击搜索结果创建的可选待定位转录目标。
    pending_navigation: Option<PendingWorkspaceHistorySearchNavigation>,
    /// Optional active transcript highlight target retained after one search result has been resolved into the visible transcript.
    /// 某条搜索结果已经成功解析到可见转录后保留的可选活动高亮目标。
    active_highlight: Option<WorkspaceHistorySearchHighlightTarget>,
}

/// Centered modal state retained while one inline thinking block is explicitly opened above the transcript.
/// 当某个内联思考区块被显式打开到转录上方时保留的居中弹层状态。
#[derive(Debug, Clone)]
struct SessionInlineAgentViewDocumentCache {
    /// Shared prepared inline transcript document reused by the main session surface without recloning the full block tree on every GUI pass.
    /// 主会话表面复用的共享内联转录文档快照，避免每次界面刷新都重新深拷贝整棵区块树。
    document: Rc<vulcan_agent_view::AgentViewDocument>,
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
    /// Generated media attachments extracted from host-only tool metadata for richer modal rendering.
    /// 从宿主专用工具元数据中提取出的生成媒体附件，用于更丰富的弹层渲染。
    generated_attachments: Vec<ConversationAttachment>,
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

/// Lifecycle states used by one cached subagent task panel.
/// 单个已缓存子代理任务面板使用的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubagentTaskRunState {
    /// The subagent is still running and only a lightweight placeholder panel should be shown.
    /// 子代理仍在运行中，此时只应显示轻量占位面板。
    Running,
    /// The subagent was stopped before natural completion and the panel should preserve the partial transcript plus the stop summary.
    /// 子代理在自然完成前已被停止，此时面板应保留部分转录与停止摘要。
    Stopped,
    /// The subagent finished successfully and its final transcript snapshot can be reviewed.
    /// 子代理已成功完成，此时可以回看其最终转录快照。
    Completed,
    /// The subagent stopped in a failed state and the panel should surface the latest failure detail.
    /// 子代理以失败状态结束，此时面板应展示最近一次失败说明。
    Failed,
}

impl SubagentTaskRunState {
    /// Returns whether the panel state still represents one active running subagent.
    /// 返回当前面板状态是否仍表示一个处于活动运行中的子代理。
    fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Active subagent panel selection shown in place of the main parent transcript.
/// 显示在主父会话转录位置上的活动子代理面板选择状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedSubagentTaskPanel {
    /// Parent session identifier that owns the subagent task card and badge entry.
    /// 拥有该子代理任务卡片与徽标入口的父会话标识。
    parent_session_id: SessionId,
    /// Stable child task identifier whose transcript panel is currently opened.
    /// 当前已打开其转录面板的稳定子任务标识。
    task_id: SessionId,
}

/// Cached subagent task panel state reconstructed from runtime delegate events and persisted task tool results.
/// 由运行时委派事件与持久化 task 工具结果重建出的子代理任务面板缓存状态。
#[derive(Debug, Clone)]
struct SubagentTaskPanelState {
    /// Stable child task identifier used by badges, cards, and panel navigation.
    /// 被徽标、卡片与面板导航共同使用的稳定子任务标识。
    task_id: SessionId,
    /// Caller-facing task identifier preserved for parent-agent reasoning and human-visible summaries.
    /// 为父代理推理与人类可见摘要保留的调用方面向任务标识。
    public_task_id: String,
    /// Parent session identifier that owns the cached subagent task panel.
    /// 拥有该缓存子代理任务面板的父会话标识。
    parent_session_id: SessionId,
    /// Best-effort subagent label shown in badges and panel headers.
    /// 展示在徽标与面板标题中的最佳努力子代理标签。
    subagent_label: String,
    /// Optional user-facing task description attached to the task invocation.
    /// 附着在任务调用上的可选面向用户任务描述。
    description: Option<String>,
    /// Current lifecycle state of the cached subagent panel.
    /// 当前缓存子代理面板所处的生命周期状态。
    run_state: SubagentTaskRunState,
    /// Best-effort final or failure text shown in the child panel summary area.
    /// 展示在子面板摘要区中的最佳努力最终结果或失败文本。
    result_text: String,
    /// Optional canonical child conversation snapshot projected into the dedicated subagent panel.
    /// 可选的规范子会话快照，会被投影到独立子代理面板中。
    conversation_state: Option<SessionConversationState>,
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

/// Active real-AI run state tracked independently for one session.
/// 为单个会话独立跟踪的活动真实 AI 运行状态。
#[derive(Debug)]
struct ActiveSessionRunState {
    /// Monotonic run identifier used to reject stale stream events for this session.
    /// 用于拒绝该会话过期流事件的单调递增运行标识。
    run_id: u64,
    /// Abort handle used to stop the session-specific in-flight AI call.
    /// 用于停止该会话专属进行中 AI 调用的中止句柄。
    handle: task::Handle,
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

/// Drop-target previews used while pending-prompt queue rows are being reordered.
/// 待发送队列行重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingPromptDropTarget {
    /// Inserts the dragged queue entry before one concrete queue row.
    /// 将正在拖拽的队列项插入到某个具体队列行之前。
    Before(String),
    /// Inserts the dragged queue entry at the tail of the current queue.
    /// 将正在拖拽的队列项插入到当前队列尾部。
    End,
}

/// Drop-target previews used while enabled AI capability targets are being reordered.
/// 已启用 AI 能力目标重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
enum AiCapabilityRouteDropTarget {
    /// Inserts the dragged capability target before one concrete enabled row.
    /// 将正在拖拽的能力目标插入到某个具体已启用行之前。
    Before(String),
    /// Inserts the dragged capability target at the end of the current capability route.
    /// 将正在拖拽的能力目标插入到当前能力路由尾部。
    End,
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

/// Blocking provider-editor modal state used when one model provider is being created or updated.
/// 新增或更新模型供应商时使用的阻塞式供应商编辑弹窗状态。
#[derive(Debug, Clone)]
struct ProviderEditorState {
    /// Whether the current editor session is creating a brand-new provider entry.
    /// 当前编辑会话是否正在创建一条全新的供应商配置。
    is_new: bool,
}

/// Blocking provider model-picker modal state used to edit one provider's enabled-model draft before explicit save.
/// 在显式保存前编辑某个供应商启用模型草稿时使用的阻塞式供应商模型配置弹窗状态。
#[derive(Debug, Clone)]
struct ProviderModelPickerState {
    /// Provider identifier whose complete catalog is currently being edited.
    /// 当前正在编辑其完整目录的供应商标识。
    selected_provider_id: String,
    /// Live free-text filter applied to the catalog list.
    /// 应用于目录列表的实时自由文本过滤值。
    filter: String,
    /// Per-provider draft enabled-model identifier sets that only become persistent after the user clicks save.
    /// 按供应商分组、仅在用户点击保存后才会真正持久化的启用模型标识草稿集合。
    draft_enabled_model_ids_by_provider: BTreeMap<String, BTreeSet<String>>,
}

/// Blocking per-model metadata editor state used to correct one enabled model's capability declaration.
/// 用于修正单个已启用模型能力声明的阻塞式逐模型元数据编辑状态。
#[derive(Debug, Clone)]
struct ProviderModelOverrideEditorState {
    /// Provider identifier owning the model currently being edited.
    /// 拥有当前编辑模型的供应商标识。
    provider_id: String,
    /// Model identifier whose override metadata is currently being edited.
    /// 当前正在编辑覆盖元数据的模型标识。
    model_id: String,
    /// Human-readable model name shown in the modal header.
    /// 显示在弹窗标题中的人类可读模型名称。
    model_name: String,
    /// Whether reasoning support should be explicitly declared by the override.
    /// 当前覆盖是否应显式声明推理能力支持。
    reasoning: bool,
    /// Whether the catalog-level attachment flag should stay declared after the override is saved.
    /// 保存覆盖后目录级附件标记是否仍应保持声明。
    attachment: bool,
    /// Whether text input should stay declared for the model.
    /// 当前模型是否仍应声明支持文本输入。
    supports_text: bool,
    /// Whether image input should stay declared for the model.
    /// 当前模型是否仍应声明支持图像输入。
    supports_image: bool,
    /// Whether audio input should stay declared for the model.
    /// 当前模型是否仍应声明支持音频输入。
    supports_audio: bool,
    /// Whether video input should stay declared for the model.
    /// 当前模型是否仍应声明支持视频输入。
    supports_video: bool,
    /// Whether PDF/file input should stay declared for the model.
    /// 当前模型是否仍应声明支持 PDF/文件输入。
    supports_pdf: bool,
    /// Editable context-window limit text buffer.
    /// 可编辑的上下文窗口上限文本缓冲区。
    context_limit_text: String,
    /// Editable input-token limit text buffer.
    /// 可编辑的输入 Token 上限文本缓冲区。
    input_limit_text: String,
    /// Editable output-token limit text buffer.
    /// 可编辑的输出 Token 上限文本缓冲区。
    output_limit_text: String,
    /// Editable multi-line JSON buffer for the optional reasoning-profile override.
    /// 可选推理档位覆盖配置使用的可编辑多行 JSON 缓冲区。
    reasoning_profile_buffer: text_editor::Content,
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

/// Shared workbench reasoning-effort alias reused by the application state and compact picker surfaces.
/// 供应用状态与紧凑选择器界面复用的共享工作台推理档位别名。
type WorkbenchReasoningEffort = ReasoningEffortLevel;

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

/// Lightweight selection payload representing one configurable MCP server entry.
/// 表示单个可配置 MCP 服务器条目的轻量选择载荷。
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

/// Default working directory seeded into new stdio MCP drafts.
/// 新建 stdio MCP 草稿时预填的默认工作目录。
const DEFAULT_MCP_WORKING_DIRECTORY: &str = "~/.vulcan/code/mcp";

/// One editable single-value row used by MCP argument and passthrough-environment lists.
/// MCP 参数与透传环境变量列表使用的单值可编辑行。
#[derive(Debug, Clone, Default)]
struct McpTextRowState {
    /// Current row value buffer.
    /// 当前行的值缓冲区。
    value: String,
}

impl McpTextRowState {
    /// Builds one single-value MCP row from an optional initial value.
    /// 根据可选初始值构建一条单值 MCP 行。
    fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

/// One editable key-value row used by MCP environment and header groups.
/// MCP 环境变量与请求头分组使用的键值可编辑行。
#[derive(Debug, Clone, Default)]
struct McpKeyValueRowState {
    /// Current row key buffer.
    /// 当前行的键缓冲区。
    key: String,
    /// Current row value buffer.
    /// 当前行的值缓冲区。
    value: String,
}

impl McpKeyValueRowState {
    /// Builds one key-value MCP row from optional initial key and value buffers.
    /// 根据可选初始键和值缓冲区构建一条 MCP 键值行。
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
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
    /// Editable argument rows shown in the settings form.
    /// 展示在设置表单中的可编辑参数行集合。
    args_rows: Vec<McpTextRowState>,
    /// Remote endpoint URL used by HTTP transports.
    /// HTTP 传输使用的远端端点 URL。
    url: String,
    /// Optional bearer token used by HTTP transports.
    /// HTTP 传输使用的可选 Bearer Token。
    bearer_token: String,
    /// Editable explicit HTTP header rows.
    /// 可编辑的显式 HTTP 请求头行集合。
    header_rows: Vec<McpKeyValueRowState>,
    /// Editable request-header rows that resolve values from process environment variables.
    /// 从当前进程环境变量解析值的可编辑请求头行集合。
    header_from_env_rows: Vec<McpKeyValueRowState>,
    /// Optional OAuth client identifier used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 客户端标识。
    oauth_client_id: String,
    /// Optional OAuth client secret used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 客户端密钥。
    oauth_client_secret: String,
    /// Delimited OAuth scope declarations shown in the settings form.
    /// 展示在设置表单中的分隔 OAuth scope 声明文本。
    oauth_scopes_text: String,
    /// Optional OAuth redirect URI used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 回调 URI。
    oauth_redirect_uri: String,
    /// Optional working directory override.
    /// 可选的工作目录覆盖值。
    cwd: String,
    /// Editable explicit environment-variable rows injected into stdio child processes.
    /// 注入到 stdio 子进程中的可编辑显式环境变量行集合。
    env_rows: Vec<McpKeyValueRowState>,
    /// Editable environment-variable names that should be passed through from the current host process.
    /// 需要从当前宿主进程透传的可编辑环境变量名称行集合。
    env_passthrough_rows: Vec<McpTextRowState>,
}

impl McpServerFormState {
    /// Returns one default stdio working directory used by new MCP drafts and empty persisted entries.
    /// 返回新 MCP 草稿与空持久化条目使用的默认 stdio 工作目录。
    fn default_working_directory() -> String {
        String::from(DEFAULT_MCP_WORKING_DIRECTORY)
    }

    /// Converts one persisted string vector into editable MCP single-value rows while preserving at least one row.
    /// 将持久化字符串向量转换为可编辑 MCP 单值行，并保证至少保留一行。
    fn text_rows_from_values(values: &[String]) -> Vec<McpTextRowState> {
        if values.is_empty() {
            vec![McpTextRowState::default()]
        } else {
            values
                .iter()
                .map(|value| McpTextRowState::new(value.clone()))
                .collect()
        }
    }

    /// Converts one persisted key-value map into editable MCP key-value rows while preserving at least one row.
    /// 将持久化键值映射转换为可编辑 MCP 键值行，并保证至少保留一行。
    fn key_value_rows_from_map(values: &BTreeMap<String, String>) -> Vec<McpKeyValueRowState> {
        if values.is_empty() {
            vec![McpKeyValueRowState::default()]
        } else {
            values
                .iter()
                .map(|(key, value)| McpKeyValueRowState::new(key.clone(), value.clone()))
                .collect()
        }
    }

    /// Resolves one ordered argument vector from the current editable row set, trimming blank entries.
    /// 从当前可编辑行集合解析出有序参数向量，并裁剪空白项。
    fn resolved_argument_rows(&self) -> Vec<String> {
        self.args_rows
            .iter()
            .map(|row| row.value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Resolves one ordered passthrough-environment list from the current editable row set, trimming blanks and removing duplicates.
    /// 从当前可编辑行集合解析出有序透传环境变量列表，并裁剪空白与移除重复项。
    fn resolved_env_passthrough_rows(&self) -> Vec<String> {
        let mut ordered_values = Vec::new();
        for value in self
            .env_passthrough_rows
            .iter()
            .map(|row| row.value.trim())
            .filter(|value| !value.is_empty())
        {
            if !ordered_values.iter().any(|existing| existing == value) {
                ordered_values.push(value.to_string());
            }
        }
        ordered_values
    }

    /// Resolves one validated key-value map from editable rows while ignoring fully blank rows and rejecting partial or duplicate keys.
    /// 从可编辑行解析出一份经过校验的键值映射，并忽略完全空白行且拒绝半填写或重复键。
    fn resolved_key_value_rows(
        rows: &[McpKeyValueRowState],
        subject_label: &str,
        value_label: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut map = BTreeMap::new();
        for row in rows {
            let key = row.key.trim();
            let value = row.value.trim();
            if key.is_empty() && value.is_empty() {
                continue;
            }
            if key.is_empty() {
                return Err(format!("{subject_label} 的键不能为空。"));
            }
            if value.is_empty() {
                return Err(format!("{subject_label} 的{value_label}不能为空。"));
            }
            if map.insert(key.to_string(), value.to_string()).is_some() {
                return Err(format!("{subject_label} `{key}` 重复，请保留一条即可。"));
            }
        }
        Ok(map)
    }

    /// Builds an editable form state from one persisted MCP server entry.
    /// 根据单个持久化 MCP 服务器条目构建可编辑表单状态。
    fn from_entry(entry: &PersistedMcpServerEntry) -> Self {
        Self {
            name: entry.name.clone(),
            enabled: entry.enabled,
            transport: entry.transport,
            command: entry.command.clone(),
            args_rows: Self::text_rows_from_values(&entry.args),
            url: entry.url.clone().unwrap_or_default(),
            bearer_token: entry.bearer_token.clone().unwrap_or_default(),
            header_rows: Self::key_value_rows_from_map(&entry.headers),
            header_from_env_rows: Self::key_value_rows_from_map(&entry.headers_from_env),
            oauth_client_id: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_id.clone())
                .unwrap_or_default(),
            oauth_client_secret: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_secret.clone())
                .unwrap_or_default(),
            oauth_scopes_text: entry
                .oauth
                .as_ref()
                .map(|oauth| join_mcp_server_oauth_scopes(&oauth.scopes))
                .unwrap_or_default(),
            oauth_redirect_uri: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.redirect_uri.clone())
                .unwrap_or_default(),
            cwd: entry
                .cwd
                .clone()
                .unwrap_or_else(Self::default_working_directory),
            env_rows: Self::key_value_rows_from_map(&entry.env),
            env_passthrough_rows: Self::text_rows_from_values(&entry.env_passthrough),
        }
    }

    /// Converts the editable form state into one persisted MCP server entry.
    /// 将可编辑表单状态转换为单个持久化 MCP 服务器条目。
    fn to_entry(&self) -> Result<PersistedMcpServerEntry, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(String::from("请先填写 MCP 服务器名称。"));
        }
        let oauth_config = self.oauth_config();

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
                    args: self.resolved_argument_rows(),
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    headers_from_env: BTreeMap::new(),
                    oauth: None,
                    cwd: trim_to_option(&self.cwd),
                    env: Self::resolved_key_value_rows(&self.env_rows, "环境变量", "值")?,
                    env_passthrough: self.resolved_env_passthrough_rows(),
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
                    headers: Self::resolved_key_value_rows(&self.header_rows, "请求头", "值")?,
                    headers_from_env: Self::resolved_key_value_rows(
                        &self.header_from_env_rows,
                        "来自环境变量的请求头",
                        "环境变量名",
                    )?,
                    oauth: oauth_config,
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                })
            }
        }
    }

    /// Resolves one persisted OAuth configuration block from the editable MCP server form when any OAuth field is currently populated.
    /// 当任一 OAuth 字段已填写时，从可编辑 MCP 服务器表单解析出一份可持久化 OAuth 配置块。
    fn oauth_config(&self) -> Option<PersistedMcpOAuthConfig> {
        let client_id = trim_to_option(&self.oauth_client_id);
        let client_secret = trim_to_option(&self.oauth_client_secret);
        let redirect_uri = trim_to_option(&self.oauth_redirect_uri);
        let scopes = parse_mcp_server_oauth_scopes(&self.oauth_scopes_text);

        if client_id.is_none()
            && client_secret.is_none()
            && redirect_uri.is_none()
            && scopes.is_empty()
        {
            return None;
        }

        Some(PersistedMcpOAuthConfig {
            client_id,
            client_secret,
            scopes,
            redirect_uri,
        })
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
            args_rows: vec![McpTextRowState::default()],
            url: String::new(),
            bearer_token: String::new(),
            header_rows: vec![McpKeyValueRowState::default()],
            header_from_env_rows: vec![McpKeyValueRowState::default()],
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_scopes_text: String::new(),
            oauth_redirect_uri: String::new(),
            cwd: Self::default_working_directory(),
            env_rows: vec![McpKeyValueRowState::default()],
            env_passthrough_rows: vec![McpTextRowState::default()],
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
            Self::Settings => "集中呈现模型、能力与运行设置，不混入全局动作。",
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

/// Settings navigation sections rendered in the left tree of the settings workspace.
/// 渲染在设置工作区左侧树导航中的设置分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    /// Chat-provider credentials and active-model configuration.
    /// 对话模型供应商凭据与已启用模型配置。
    ModelProviders,
    /// Enabled-model overview grouped by provider with per-model capability editing.
    /// 按供应商分组的已启用模型总览与逐模型能力编辑入口。
    ModelList,
    /// Runtime tuning, execution diagnostics, and engineering guardrails.
    /// 运行调优、执行诊断与工程约束设置。
    RuntimeDiagnostics,
    /// Shared AI capability vendor credentials reused by multiple semantic tools.
    /// 被多个语义工具复用的共享 AI 能力供应商凭据。
    AiCapabilityVendors,
    /// Unified hosted or third-party web-search capability routing.
    /// 统一原生或第三方网页搜索能力路由。
    AiCapabilityWebSearch,
    /// Image-understanding capability routing delegated to external vision vendors.
    /// 委派给外部视觉供应商的图片理解能力路由。
    AiCapabilityUnderstandImage,
    /// Video-understanding capability routing placeholder reserved for future multimodal ingestion.
    /// 为未来多模态接入预留的视频理解能力路由占位。
    AiCapabilityUnderstandVideo,
    /// Audio-understanding capability routing placeholder reserved for future multimodal ingestion.
    /// 为未来多模态接入预留的音频理解能力路由占位。
    AiCapabilityUnderstandAudio,
    /// Image-generation capability routing.
    /// 图片生成能力路由。
    AiCapabilityImageGeneration,
    /// Video-generation capability routing.
    /// 视频生成能力路由。
    AiCapabilityVideoGeneration,
    /// Speech-synthesis capability routing.
    /// 语音合成能力路由。
    AiCapabilitySpeechSynthesis,
    /// Music-generation capability routing.
    /// 音乐生成能力路由。
    AiCapabilityMusicGeneration,
    /// Music-cover capability routing.
    /// 音乐翻唱能力路由。
    AiCapabilityMusicCover,
    /// MCP server configuration and OAuth management.
    /// MCP 服务器配置与 OAuth 管理。
    McpServers,
    /// Archived session and project restoration surface.
    /// 归档会话与归档项目恢复界面。
    ArchivedRestore,
}

impl SettingsSection {
    /// Returns the compact tree label rendered in the left settings navigation.
    /// 返回渲染在左侧设置导航中的紧凑树标签。
    const fn label(self) -> &'static str {
        match self {
            Self::ModelProviders => "模型供应商",
            Self::ModelList => "模型列表",
            Self::RuntimeDiagnostics => "运行与诊断",
            Self::AiCapabilityVendors => "能力供应商",
            Self::AiCapabilityWebSearch => "联网搜索",
            Self::AiCapabilityUnderstandImage => "图像理解",
            Self::AiCapabilityUnderstandVideo => "视频理解",
            Self::AiCapabilityUnderstandAudio => "音频理解",
            Self::AiCapabilityImageGeneration => "图像生成",
            Self::AiCapabilityVideoGeneration => "视频生成",
            Self::AiCapabilitySpeechSynthesis => "语音合成",
            Self::AiCapabilityMusicGeneration => "音乐生成",
            Self::AiCapabilityMusicCover => "音乐翻唱",
            Self::McpServers => "MCP 配置",
            Self::ArchivedRestore => "归档恢复",
        }
    }

    /// Returns the settings-page title rendered above the selected section content.
    /// 返回渲染在所选分区内容上方的设置页标题。
    const fn title(self) -> &'static str {
        self.label()
    }

    /// Returns one concise description for the selected settings section.
    /// 返回所选设置分区的一句简洁说明。
    const fn description(self) -> &'static str {
        match self {
            Self::ModelProviders => {
                "管理对话模型供应商接入配置，并通过配置动作进入模型启用草稿页。"
            }
            Self::ModelList => "管理已启用模型与模型能力覆盖。",
            Self::RuntimeDiagnostics => "集中查看运行参数、执行运行时、工程策略、指引与日志。",
            Self::AiCapabilityVendors => "先在这里配置共享能力供应商凭据，后续各能力页直接复用。",
            Self::AiCapabilityWebSearch => "管理统一 web-search 的默认路由、启用接口与顺序。",
            Self::AiCapabilityUnderstandImage => "为不具备原生视觉的模型补齐图片理解能力。",
            Self::AiCapabilityUnderstandVideo => {
                "预留未来的视频理解入口；当前仅提供配置入口与路由占位。"
            }
            Self::AiCapabilityUnderstandAudio => {
                "预留未来的音频理解入口；当前仅提供配置入口与路由占位。"
            }
            Self::AiCapabilityImageGeneration => "配置可供对话模型调用的图片生成目标与顺序。",
            Self::AiCapabilityVideoGeneration => "配置可供对话模型调用的视频生成目标与顺序。",
            Self::AiCapabilitySpeechSynthesis => "配置文本转语音与语音合成目标与顺序。",
            Self::AiCapabilityMusicGeneration => "配置音乐生成目标与默认顺序。",
            Self::AiCapabilityMusicCover => "配置音乐翻唱目标与默认顺序。",
            Self::McpServers => "管理 MCP Server、OAuth 授权与连接参数。",
            Self::ArchivedRestore => "在设置页恢复已经归档的项目与会话。",
        }
    }

    /// Returns whether the current section belongs to the collapsible AI capability subtree.
    /// 返回当前分区是否属于可折叠的 AI 能力子树。
    const fn belongs_to_ai_capabilities(self) -> bool {
        matches!(
            self,
            Self::AiCapabilityVendors
                | Self::AiCapabilityWebSearch
                | Self::AiCapabilityUnderstandImage
                | Self::AiCapabilityUnderstandVideo
                | Self::AiCapabilityUnderstandAudio
                | Self::AiCapabilityImageGeneration
                | Self::AiCapabilityVideoGeneration
                | Self::AiCapabilitySpeechSynthesis
                | Self::AiCapabilityMusicGeneration
                | Self::AiCapabilityMusicCover
        )
    }

    /// Returns whether the current section belongs to the collapsible model-settings subtree.
    /// 返回当前分区是否属于可折叠的模型设置子树。
    const fn belongs_to_model_settings(self) -> bool {
        matches!(self, Self::ModelProviders | Self::ModelList)
    }

    /// Returns the semantic capability kind represented by one settings section when the section manages one concrete capability.
    /// 返回某个设置分区所代表的语义能力类型；若该分区并不管理具体能力则返回空值。
    const fn capability_kind(self) -> Option<AiToolKind> {
        match self {
            Self::AiCapabilityWebSearch => Some(AiToolKind::WebSearch),
            Self::AiCapabilityUnderstandImage => Some(AiToolKind::UnderstandImage),
            Self::AiCapabilityUnderstandVideo => None,
            Self::AiCapabilityUnderstandAudio => None,
            Self::AiCapabilityImageGeneration => Some(AiToolKind::ImageGeneration),
            Self::AiCapabilityVideoGeneration => Some(AiToolKind::VideoGeneration),
            Self::AiCapabilitySpeechSynthesis => Some(AiToolKind::SpeechSynthesis),
            Self::AiCapabilityMusicGeneration => Some(AiToolKind::MusicGeneration),
            Self::AiCapabilityMusicCover => Some(AiToolKind::MusicCover),
            _ => None,
        }
    }

    /// Returns whether the current settings section should expose a compact routing-strategy selector.
    /// 返回当前设置分区是否应暴露紧凑路由策略选择器。
    const fn supports_routing_strategy(self) -> bool {
        matches!(
            self,
            Self::AiCapabilityWebSearch | Self::AiCapabilityUnderstandImage
        )
    }
}

/// Capability sections rendered inside the workbench quick capability panel opened from the composer controls.
/// 渲染在提示词控制区快捷能力面板中的能力分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkbenchAiCapabilitySection {
    /// Unified hosted or third-party web-search capability routing.
    /// 统一原生或第三方网页搜索能力路由。
    WebSearch,
    /// Image-understanding capability routing delegated to external vision vendors.
    /// 委派给外部视觉供应商的图片理解能力路由。
    UnderstandImage,
    /// Video-understanding placeholder section.
    /// 视频理解占位分区。
    UnderstandVideo,
    /// Audio-understanding placeholder section.
    /// 音频理解占位分区。
    UnderstandAudio,
    /// Image-generation capability routing.
    /// 图片生成能力路由。
    ImageGeneration,
    /// Video-generation capability routing.
    /// 视频生成能力路由。
    VideoGeneration,
    /// Speech-synthesis capability routing.
    /// 语音合成能力路由。
    SpeechSynthesis,
    /// Music-generation capability routing.
    /// 音乐生成能力路由。
    MusicGeneration,
    /// Music-cover capability routing.
    /// 音乐翻唱能力路由。
    MusicCover,
}

impl WorkbenchAiCapabilitySection {
    /// Returns all quick-panel sections in stable order.
    /// 返回快捷能力面板按稳定顺序展示的全部分区。
    const ALL: [Self; 9] = [
        Self::WebSearch,
        Self::UnderstandImage,
        Self::UnderstandVideo,
        Self::UnderstandAudio,
        Self::ImageGeneration,
        Self::VideoGeneration,
        Self::SpeechSynthesis,
        Self::MusicGeneration,
        Self::MusicCover,
    ];

    /// Returns the compact function label rendered in the quick capability panel.
    /// 返回渲染在快捷能力面板中的紧凑功能标签。
    const fn label(self) -> &'static str {
        match self {
            Self::WebSearch => "联网搜索",
            Self::UnderstandImage => "图像理解",
            Self::UnderstandVideo => "视频理解",
            Self::UnderstandAudio => "音频理解",
            Self::ImageGeneration => "图像生成",
            Self::VideoGeneration => "视频生成",
            Self::SpeechSynthesis => "语音合成",
            Self::MusicGeneration => "音乐生成",
            Self::MusicCover => "音乐翻唱",
        }
    }

    /// Returns the semantic capability kind already implemented by the current section when one exists.
    /// 返回当前分区已经具备实现时对应的语义能力类型。
    const fn capability_kind(self) -> Option<AiToolKind> {
        match self {
            Self::WebSearch => Some(AiToolKind::WebSearch),
            Self::UnderstandImage => Some(AiToolKind::UnderstandImage),
            Self::UnderstandVideo => None,
            Self::UnderstandAudio => None,
            Self::ImageGeneration => Some(AiToolKind::ImageGeneration),
            Self::VideoGeneration => Some(AiToolKind::VideoGeneration),
            Self::SpeechSynthesis => Some(AiToolKind::SpeechSynthesis),
            Self::MusicGeneration => Some(AiToolKind::MusicGeneration),
            Self::MusicCover => Some(AiToolKind::MusicCover),
        }
    }

    /// Returns whether the current section should show one routing-strategy block above the target list.
    /// 返回当前分区是否应在目标列表上方展示路由策略区块。
    const fn supports_routing_strategy(self) -> bool {
        matches!(
            self,
            Self::WebSearch | Self::UnderstandImage | Self::UnderstandVideo | Self::UnderstandAudio
        )
    }
}

/// Temporary in-memory picker state used by one capability page when the user selects targets from configured vendors.
/// 某个能力页在用户从已配置供应商中选择目标时使用的临时内存选择器状态。
#[derive(Debug, Clone)]
struct AiCapabilityTargetPickerState {
    /// Semantic capability currently being configured by the picker.
    /// 当前由选择器配置的语义能力。
    capability_kind: AiToolKind,
    /// Vendor currently highlighted in the left-side picker list.
    /// 当前在选择器左侧列表中高亮的供应商。
    selected_vendor_id: Option<String>,
    /// Per-vendor selected target map keyed by `target_alias` with its requested model override.
    /// 以 `target_alias` 为键、以请求模型覆盖值为值的按供应商分组已选目标映射。
    selections: BTreeMap<String, BTreeMap<String, String>>,
}

impl AiCapabilityTargetPickerState {
    /// Returns the total number of checked targets currently staged inside the picker.
    /// 返回当前在选择器中暂存勾选的目标总数。
    fn total_selected_count(&self) -> usize {
        self.selections.values().map(BTreeMap::len).sum()
    }
}

/// Temporary in-memory picker state used by one capability page when the user chooses a routing strategy from a compact selector.
/// 某个能力页在用户从紧凑选择器中挑选路由策略时使用的临时内存弹窗状态。
#[derive(Debug, Clone)]
struct AiCapabilityRoutingPickerState {
    /// Semantic capability currently owning the routing strategy being edited.
    /// 当前持有待编辑路由策略的语义能力。
    capability_kind: AiToolKind,
}

/// Blocking vendor picker state used by the supplier page before one new shared AI capability vendor instance is created.
/// 供应商页在创建新的共享 AI 能力供应商实例前使用的阻塞式供应商选择器状态。
#[derive(Debug, Clone, Default)]
struct AiCapabilityVendorPickerState {
    /// Live keyword filter applied to the vendor catalog list.
    /// 应用于供应商目录列表的实时关键字过滤值。
    filter: String,
}

/// Blocking vendor editor state used when one shared AI capability vendor instance is added or updated.
/// 新增或更新共享 AI 能力供应商实例时使用的阻塞式供应商编辑器状态。
#[derive(Debug, Clone)]
struct AiCapabilityVendorEditorState {
    /// Stable shared vendor identifier currently being edited.
    /// 当前正在编辑的稳定共享供应商标识。
    vendor_id: String,
    /// Shared vendor display name shown in the modal header.
    /// 显示在弹窗标题中的共享供应商名称。
    display_name: String,
    /// Shared API key or token staged in the editor.
    /// 在编辑器中暂存的共享 API Key 或 Token。
    api_key: String,
    /// Optional local/cloud connection mode staged in the editor for vendors that support explicit host switching.
    /// 为支持显式宿主切换的供应商在编辑器中暂存的本地/云端连接模式。
    connection_mode: crate::provider_config::AiCapabilityVendorConnectionMode,
    /// Shared vendor-level base URL override staged in the editor.
    /// 在编辑器中暂存的共享供应商级 Base URL 覆盖值。
    base_url: String,
    /// Shared region or locale staged in the editor.
    /// 在编辑器中暂存的共享区域或地区值。
    region: String,
    /// Optional fixed region choices rendered as a dropdown when the provider only supports a known finite set.
    /// 当提供方只支持已知有限集合时渲染为下拉框的可选固定区域列表。
    region_options: Vec<String>,
    /// Whether the current editor session is creating a brand-new vendor entry instead of updating an existing one.
    /// 当前编辑会话是否正在创建全新供应商条目，而不是更新已有条目。
    is_new: bool,
}

/// Session-scoped agent-view projection snapshot materialized off the UI thread before being applied back into the live workbench caches.
/// 在 UI 线程之外先行物化、随后再回写到工作台实时缓存中的按会话归属 agent-view 投影视图快照。
#[derive(Debug, Clone)]
struct SessionAgentViewProjectionSnapshot {
    /// Stable session identifier owning the projected transcript payloads.
    /// 拥有当前投影转录载荷的稳定会话标识。
    session_id: SessionId,
    /// Structured transcript document rendered by the unified agent-view surface.
    /// 由统一 agent-view 表面渲染的结构化转录文档。
    document: vulcan_agent_view::AgentViewDocument,
    /// Selectable projection aligned with the structured document and reused by selection-aware transcript surfaces.
    /// 与结构化文档对齐、并供支持文本选择的转录表面复用的可选投影对象。
    selectable_document: vulcan_agent_view::AgentViewSelectableDocument,
    /// Plain selectable transcript text stored into the read-only text-editor buffer cache.
    /// 保存到只读文本编辑缓冲区缓存中的可选转录纯文本。
    selectable_text: String,
    /// Raw-source projection stored into the dedicated source-view buffer cache.
    /// 保存到专用源码视图缓冲区缓存中的原始源码投影文本。
    raw_source_text: String,
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
    McpRuntimeBootstrapped(Result<McpRuntimeAccess, String>),
    /// Applies the completed workspace-state load result after the shared runtimes become ready.
    /// 应用共享运行时就绪后已完成的工作台状态加载结果。
    WorkspaceStateLoaded(Result<WorkspaceSnapshot, String>),
    /// Applies the completed lazy hydration result for one opened session.
    /// 应用单个已打开会话的懒加载水合结果。
    SessionHydrationLoaded(SessionId, Result<SessionHydrationSnapshot, String>),
    /// Applies one background-built unified transcript projection snapshot for the given session.
    /// 应用指定会话在后台构建完成的一份统一转录投影视图快照。
    SessionAgentViewProjectionLoaded(
        SessionId,
        Result<SessionAgentViewProjectionSnapshot, String>,
    ),
    /// Applies one older transcript page loaded before the current session window.
    /// 应用在当前会话窗口之前加载的一页更早转录历史。
    OlderSessionHistoryLoaded(SessionId, Result<SessionHistoryPage, String>),
    /// Applies one lazily loaded tool-detail page for a hydrated session.
    /// 应用已水合会话的一页懒加载工具明细。
    SessionToolDetailsLoaded(SessionId, Result<SessionToolDetailPage, String>),
    /// Applies the completed legacy-storage maintenance result for one hydrated session.
    /// 应用单个已水合会话的旧存储维护结果。
    SessionMaintenanceFinished(SessionId, Result<SessionMaintenanceReport, String>),
    /// Applies the completed background legacy-storage maintenance result for one session.
    /// 应用单个会话的后台旧存储维护结果。
    BackgroundLegacySessionMaintenanceFinished(SessionId, Result<SessionMaintenanceReport, String>),
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
    /// Tracks the latest keyboard modifier state reported by the window event stream.
    /// 跟踪窗口事件流回传的最新键盘修饰键状态。
    WorkbenchKeyboardModifiersChanged(keyboard::Modifiers),
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
    /// Restores the previous composer snapshot requested by the undo shortcut.
    /// 恢复由撤销快捷键请求的上一条提示词快照。
    ComposerUndo,
    /// Opens the native file picker and appends one or more supported attachments into the composer.
    /// 打开原生文件选择器并向提示词输入框追加一个或多个受支持附件。
    AddComposerAttachments,
    /// Smart-pastes clipboard contents into the composer by preferring file attachments, then in-memory images, and finally plain text.
    /// 按“文件附件、内存图片、普通文本”的优先顺序，将剪贴板内容智能粘贴到提示词输入框。
    PasteComposerClipboardContents,
    /// Pastes file or image attachments from the current clipboard into the composer.
    /// 将当前剪贴板中的文件或图片附件粘贴到提示词输入框。
    PasteComposerClipboardAttachments,
    /// Removes one staged composer attachment identified by its canonical local path.
    /// 根据规范本地路径移除一条已暂存的提示词附件。
    RemoveComposerAttachment(String),
    /// Opens the blocking image preview modal for one staged image attachment.
    /// 为某个已暂存图片附件打开阻塞式图片预览弹窗。
    OpenComposerImageAttachmentPreview {
        /// File name shown in the image preview modal header.
        /// 显示在图片预览弹窗标题栏中的文件名。
        file_name: String,
        /// Local image path rendered by the image preview modal.
        /// 由图片预览弹窗渲染的本地图片路径。
        local_path: String,
    },
    /// Opens one generated media file with the host operating system default application.
    /// 使用宿主操作系统默认应用打开某个生成媒体文件。
    OpenGeneratedAttachmentWithDefaultApp(String),
    /// Reveals one generated media file in the host operating system file manager.
    /// 在宿主操作系统文件管理器中定位某个生成媒体文件。
    RevealGeneratedAttachmentInFolder(String),
    /// Copies one generated media file path into the system clipboard.
    /// 将某个生成媒体文件路径复制到系统剪贴板。
    CopyGeneratedAttachmentPath(String),
    /// Toggles host-side playback for one generated audio file.
    /// 为某个生成音频文件切换宿主侧播放状态。
    ToggleGeneratedAudioPlayback {
        /// Human-readable file name used by status text and notices.
        /// 用于状态文本与提示弹窗的人类可读文件名。
        file_name: String,
        /// Local absolute path of the generated audio file.
        /// 生成音频文件的本地绝对路径。
        local_path: String,
    },
    /// Copies one generated image file into the system clipboard as bitmap content.
    /// 将某个生成图片文件以位图内容形式复制到系统剪贴板。
    CopyGeneratedImageToClipboard {
        /// Human-readable file name used by status text and notices.
        /// 用于状态文本与提示弹窗的人类可读文件名。
        file_name: String,
        /// Local absolute path of the generated image file.
        /// 生成图片文件的本地绝对路径。
        local_path: String,
    },
    /// Closes the currently visible image preview modal.
    /// 关闭当前可见的图片预览弹窗。
    CloseWorkbenchImagePreviewModal,
    /// Adjusts the current image preview zoom factor by one signed delta.
    /// 按一个有符号增量调整当前图片预览缩放倍率。
    AdjustWorkbenchImagePreviewZoom(f32),
    /// Handles one captured mouse-wheel movement while the image preview modal is visible.
    /// 处理图片预览弹窗可见时捕获到的一次鼠标滚轮移动。
    WorkbenchImagePreviewWheelScrolled(iced::mouse::ScrollDelta),
    /// Starts one left-button pan interaction inside the image preview surface.
    /// 在图片预览区域内部启动一次鼠标左键平移交互。
    WorkbenchImagePreviewLeftPressed,
    /// Ends the active left-button pan interaction inside the image preview surface.
    /// 结束图片预览区域内部当前激活的鼠标左键平移交互。
    WorkbenchImagePreviewLeftReleased,
    /// Tracks the latest local pointer position inside the image preview surface.
    /// 跟踪图片预览区域内部最近一次局部鼠标位置。
    WorkbenchImagePreviewPointerMoved(iced::Point),
    /// Tracks the latest scrollable viewport emitted by the image preview surface.
    /// 跟踪图片预览区域滚动容器最近一次回传的视口状态。
    WorkbenchImagePreviewScrolled(scrollable::Viewport),
    /// Resets the current image preview zoom factor to its default fit value.
    /// 将当前图片预览缩放倍率还原到默认适配值。
    ResetWorkbenchImagePreviewZoom,
    /// Confirms and closes the currently visible reusable workbench notice modal.
    /// 确认并关闭当前可见的可复用工作台提示弹窗。
    ConfirmWorkbenchNoticeModal,
    /// Closes the currently visible reusable workbench notice modal without changing domain state.
    /// 关闭当前可见的可复用工作台提示弹窗且不修改业务状态。
    CloseWorkbenchNoticeModal,
    /// Dismisses the top-most keyboard-driven overlay, preferring the reusable notice modal before legacy overlays.
    /// 关闭键盘驱动的最顶层遮罩，优先处理可复用提示弹窗再回退到旧弹层。
    DismissTopLevelOverlay,
    /// Confirms the top-most keyboard-driven overlay, preferring the reusable notice modal before model selection.
    /// 确认键盘驱动的最顶层遮罩，优先处理可复用提示弹窗再回退到模型选择。
    ConfirmTopLevelOverlay,
    /// Tracks one supported or unsupported file currently hovering over the main window as a potential composer drop target.
    /// 跟踪当前悬停在主窗口上、可作为提示词拖放目标的一条受支持或不受支持文件。
    WindowFileHovered(window::Id, PathBuf),
    /// Reports that hovered files have left the target window and the composer drop preview should clear.
    /// 回传悬停文件已离开目标窗口，提示词拖放预览应被清空。
    WindowFilesHoverLeft(window::Id),
    /// Reports that one file has been dropped onto the target window for composer attachment import.
    /// 回传某个文件已被拖放到目标窗口，供提示词附件导入使用。
    WindowFileDropped(window::Id, PathBuf),
    /// Opens the composer context menu at the latest tracked cursor position.
    /// 在最近一次追踪到的鼠标位置打开提示词上下文菜单。
    OpenComposerContextMenu,
    /// Opens the shared context menu for one visible task project row.
    /// 为一条可见任务项目行打开共享右键菜单。
    OpenTaskProjectContextMenu(String),
    /// Opens the shared context menu for one visible task session row.
    /// 为一条可见任务会话行打开共享右键菜单。
    OpenTaskSessionContextMenu(SessionId),
    /// Opens the shared context menu for one archived task project row.
    /// 为一条已归档任务项目行打开共享右键菜单。
    OpenArchivedTaskProjectContextMenu(String),
    /// Opens the shared context menu for one archived task session row.
    /// 为一条已归档任务会话行打开共享右键菜单。
    OpenArchivedTaskSessionContextMenu(SessionId),
    /// Opens the shared context menu requested by one selectable transcript surface.
    /// 打开某个可选择转录表面请求的共享右键菜单。
    OpenTranscriptContextMenu(SelectableTranscriptContextMenuRequest),
    /// Closes the current shared workbench context menu without invoking an item.
    /// 在不触发菜单项的情况下关闭当前共享工作台右键菜单。
    CloseWorkbenchContextMenu,
    /// Invokes one item from the currently open shared workbench context menu.
    /// 触发当前已打开共享工作台右键菜单中的某一项。
    InvokeWorkbenchContextMenuItem(usize),
    /// Copies the current composer selection through the context menu.
    /// 通过上下文菜单复制当前提示词选区。
    ComposerContextMenuCopy,
    /// Cuts the current composer selection through the context menu.
    /// 通过上下文菜单剪切当前提示词选区。
    ComposerContextMenuCut,
    /// Pastes the current clipboard text through the context menu.
    /// 通过上下文菜单粘贴当前剪贴板文本。
    ComposerContextMenuPaste,
    /// Selects all composer contents through the context menu.
    /// 通过上下文菜单全选提示词内容。
    ComposerContextMenuSelectAll,
    /// Copies the selected transcript text captured when its context menu opened.
    /// 复制打开转录右键菜单时捕获的转录选中文本。
    TranscriptContextMenuCopySelection(String),
    /// Selects all text in the transcript surface that opened the shared context menu.
    /// 全选打开共享右键菜单的转录表面中的文本。
    TranscriptContextMenuSelectAll(SelectableTranscriptSurfaceId),
    /// Applies the clipboard text snapshot loaded for the composer context menu.
    /// 应用为提示词上下文菜单载入的剪贴板文本快照。
    ComposerContextClipboardLoaded(Result<std::sync::Arc<String>, clipboard::Error>),
    /// Updates the latest local pointer y position tracked inside the composer editor surface.
    /// 更新在提示词编辑器表面内部追踪到的最新局部纵向指针位置。
    ComposerScrollbarPointerMoved(f32),
    /// Starts one scrollbar drag interaction using the latest tracked pointer position.
    /// 使用最近一次追踪到的指针位置启动一次滚动条拖拽交互。
    StartComposerScrollbarDrag,
    /// Embedded editor emitted an editing action.
    /// 嵌入式编辑器发出了编辑动作。
    EditorActionPerformed(text_editor::Action),
    /// Supported provider selected from the provider catalog picker.
    /// 从供应商目录选择框中选中了某个受支持供应商。
    ProviderCatalogSelected(ProviderChoice),
    /// Selects one dedicated section inside the settings workspace.
    /// 选中设置工作区中的某个独立分区。
    SettingsSectionSelected(SettingsSection),
    /// Toggles whether the AI capability subtree stays expanded in the left settings tree.
    /// 切换左侧设置树中的 AI 能力子树是否保持展开。
    ToggleAiCapabilitySettingsTree,
    /// Toggles whether the model-settings subtree stays expanded in the left settings tree.
    /// 切换左侧设置树中的模型设置子树是否保持展开。
    ToggleModelSettingsTree,
    /// One provider form field changed in the configuration page.
    /// 配置页面中的某个供应商表单字段发生变化。
    ProviderFieldChanged(ProviderFieldKey, String),
    /// Opens the blocking editor used to create one brand-new provider configuration entry.
    /// 打开用于创建全新供应商配置条目的阻塞式编辑弹窗。
    OpenProviderEditorForCreate,
    /// Opens the blocking editor used to update one existing configured provider entry.
    /// 打开用于更新某条已有供应商配置的阻塞式编辑弹窗。
    OpenProviderEditorForEdit(String),
    /// Closes the currently visible provider editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的供应商编辑弹窗。
    CloseProviderEditor,
    /// Persists the currently selected provider configuration.
    /// 持久化当前选中供应商的配置。
    SaveProviderConfiguration,
    /// Opens the blocking model-picker modal used to edit one configured provider's enabled-model draft.
    /// 打开用于编辑某个已配置供应商启用模型草稿的阻塞式模型配置弹窗。
    OpenProviderModelPicker(String),
    /// Closes the currently visible provider model-picker modal without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的供应商模型配置弹窗。
    CloseProviderModelPicker,
    /// Selects one provider row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部选中某条供应商行。
    ProviderModelPickerProviderSelected(String),
    /// Updates the live filter text used by the blocking provider model-picker modal.
    /// 更新阻塞式供应商模型配置弹窗使用的实时过滤文本。
    ProviderModelPickerFilterChanged(String),
    /// Toggles one staged enabled-model checkbox inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部切换某个暂存启用模型勾选框。
    ProviderModelPickerModelToggled {
        /// Provider identifier owning the staged catalog row.
        /// 拥有该暂存目录行的供应商标识。
        provider_id: String,
        /// Model identifier being toggled in the staged enabled-model set.
        /// 当前在暂存启用模型集合中被切换的模型标识。
        model_id: String,
        /// Whether the staged model should stay enabled after the toggle.
        /// 当前暂存模型在切换后是否应保持启用。
        enabled: bool,
    },
    /// Selects every currently visible model row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部选中当前全部可见模型行。
    ProviderModelPickerSelectAllVisible,
    /// Clears every currently visible model row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部取消当前全部可见模型行。
    ProviderModelPickerClearAllVisible,
    /// Persists the staged enabled-model draft from the blocking provider model-picker modal.
    /// 将阻塞式供应商模型配置弹窗中的启用模型草稿持久化保存。
    SaveProviderModelPicker,
    /// Removes one configured provider entry chosen from the provider result list.
    /// 从供应商结果列表中移除一条已配置供应商条目。
    RemoveConfiguredProvider(String),
    /// Opens the blocking per-model metadata editor used to correct one enabled model entry.
    /// 打开用于修正某条已启用模型元数据的阻塞式逐模型编辑弹窗。
    OpenProviderModelOverrideEditor {
        /// Provider identifier owning the selected model.
        /// 拥有当前所选模型的供应商标识。
        provider_id: String,
        /// Model identifier whose override metadata should be edited.
        /// 需要编辑其覆盖元数据的模型标识。
        model_id: String,
    },
    /// Closes the currently visible per-model metadata editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的逐模型元数据编辑弹窗。
    CloseProviderModelOverrideEditor,
    /// Updates the staged reasoning support flag inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存推理能力标记。
    ProviderModelOverrideReasoningChanged(bool),
    /// Updates the staged attachment declaration flag inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存附件声明标记。
    ProviderModelOverrideAttachmentChanged(bool),
    /// Toggles one staged input modality flag inside the per-model metadata editor.
    /// 在逐模型元数据编辑弹窗中切换某个暂存输入模态标记。
    ProviderModelOverrideInputModalityToggled {
        /// Normalized modality name being toggled.
        /// 当前被切换的规范化模态名称。
        modality: String,
        /// Whether the staged modality should stay enabled.
        /// 当前暂存模态在切换后是否应保持启用。
        enabled: bool,
    },
    /// Updates the staged context-window limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存上下文窗口上限文本。
    ProviderModelOverrideContextLimitChanged(String),
    /// Updates the staged input-token limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存输入 Token 上限文本。
    ProviderModelOverrideInputLimitChanged(String),
    /// Updates the staged output-token limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存输出 Token 上限文本。
    ProviderModelOverrideOutputLimitChanged(String),
    /// Applies one text-editor action inside the staged reasoning-profile JSON editor.
    /// 在暂存推理档位 JSON 编辑器中应用一条文本编辑器动作。
    ProviderModelOverrideReasoningProfileAction(text_editor::Action),
    /// Persists the staged per-model override metadata back into the provider configuration file.
    /// 将暂存的逐模型覆盖元数据持久化回供应商配置文件。
    SaveProviderModelOverrideEditor,
    /// Selects the global web-search routing mode.
    /// 选择全局网页搜索路由模式。
    WebSearchRoutingModeSelected(WebSearchRoutingMode),
    /// Selects the global native-versus-host routing mode for image understanding.
    /// 选择图像理解的全局原生与宿主桥接路由模式。
    UnderstandImageRoutingModeSelected(NativeBridgeRoutingMode),
    /// Opens the compact routing-strategy picker used by one concrete capability settings page.
    /// 打开某个具体能力设置页使用的紧凑路由策略选择弹窗。
    OpenAiCapabilityRoutingPicker(AiToolKind),
    /// Closes the currently visible compact routing-strategy picker without changing the active mode.
    /// 在不修改当前模式的前提下关闭当前可见的紧凑路由策略选择弹窗。
    CloseAiCapabilityRoutingPicker,
    /// Toggles one configured semantic capability target directly from the settings page or quick capability panel.
    /// 直接从设置页或快捷能力面板切换某条已配置语义能力目标的启用状态。
    AiCapabilityTargetEnabledToggled {
        /// Semantic capability owning the target row.
        /// 拥有该目标行的语义能力。
        capability_kind: AiToolKind,
        /// Stable vendor identifier owning the target.
        /// 拥有该目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias being toggled.
        /// 当前切换的稳定目标别名。
        target_alias: String,
        /// Whether the configured target should stay enabled.
        /// 当前已配置目标是否应保持启用。
        enabled: bool,
    },
    /// Removes one shared AI capability vendor instance.
    /// 移除一条共享 AI 能力供应商实例。
    RemoveAiCapabilityVendor(String),
    /// Opens the vendor picker used by the supplier page before one new shared AI capability vendor is added.
    /// 打开供应商页在新增共享 AI 能力供应商前使用的供应商选择器。
    OpenAiCapabilityVendorPicker,
    /// Closes the currently visible AI capability vendor picker without creating any vendor.
    /// 关闭当前可见的 AI 能力供应商选择器，且不创建任何供应商。
    CloseAiCapabilityVendorPicker,
    /// Updates the live keyword filter shown inside the AI capability vendor picker.
    /// 更新 AI 能力供应商选择器中展示的实时关键字过滤值。
    AiCapabilityVendorPickerFilterChanged(String),
    /// Chooses one vendor from the AI capability vendor picker and opens the dedicated vendor editor.
    /// 从 AI 能力供应商选择器中选中一个供应商并打开独立供应商编辑器。
    AiCapabilityVendorPickerVendorSelected(String),
    /// Opens the dedicated editor for one already configured shared AI capability vendor.
    /// 为一个已配置的共享 AI 能力供应商打开独立编辑器。
    OpenAiCapabilityVendorEditor(String),
    /// Closes the currently visible shared AI capability vendor editor without saving staged changes.
    /// 关闭当前可见的共享 AI 能力供应商编辑器且不保存暂存改动。
    CloseAiCapabilityVendorEditor,
    /// Updates the staged API key value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存 API Key 值。
    AiCapabilityVendorEditorApiKeyChanged(String),
    /// Updates the staged local/cloud connection mode inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存本地/云端连接模式。
    AiCapabilityVendorEditorConnectionModeChanged(
        crate::provider_config::AiCapabilityVendorConnectionMode,
    ),
    /// Updates the staged Base URL value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存 Base URL 值。
    AiCapabilityVendorEditorBaseUrlChanged(String),
    /// Updates the staged region value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存区域值。
    AiCapabilityVendorEditorRegionChanged(String),
    /// Persists the staged shared AI capability vendor editor values back into the dedicated vendor list.
    /// 将暂存的共享 AI 能力供应商编辑器内容持久化回独立供应商列表。
    SaveAiCapabilityVendorEditor,
    /// Opens the batch target-picker modal for one semantic AI capability page.
    /// 为某个语义 AI 能力页打开批量目标选择弹窗。
    OpenAiCapabilityTargetPicker(AiToolKind),
    /// Closes the currently visible AI capability target-picker modal without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的 AI 能力目标选择弹窗。
    CloseAiCapabilityTargetPicker,
    /// Selects one vendor row inside the AI capability target-picker modal.
    /// 在 AI 能力目标选择弹窗中选中某一条供应商行。
    AiCapabilityTargetPickerVendorSelected(String),
    /// Toggles one staged target checkbox inside the AI capability target-picker modal.
    /// 在 AI 能力目标选择弹窗中切换某个暂存目标勾选框。
    AiCapabilityTargetPickerTargetToggled {
        /// Stable vendor identifier owning the staged target.
        /// 拥有该暂存目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias being toggled.
        /// 当前切换的稳定目标别名。
        target_alias: String,
        /// Whether the staged target should stay selected.
        /// 当前暂存目标是否应保持选中。
        enabled: bool,
    },
    /// Updates the staged request model attached to one selected target inside the AI capability target-picker modal.
    /// 更新 AI 能力目标选择弹窗中某个已选目标绑定的暂存请求模型。
    AiCapabilityTargetPickerTargetModelChanged {
        /// Stable vendor identifier owning the staged target.
        /// 拥有该暂存目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias whose model is being edited.
        /// 当前正在编辑模型的稳定目标别名。
        target_alias: String,
        /// Updated staged provider request model identifier.
        /// 更新后的暂存 Provider 请求模型标识。
        model_id: String,
    },
    /// Persists the current staged capability target selections back into the shared vendor-centric AI capability configuration.
    /// 将当前暂存的能力目标选择持久化回共享供应商中心 AI 能力配置。
    SaveAiCapabilityTargetPicker,
    /// Starts one drag interaction from an enabled AI capability target row handle.
    /// 从某条已启用 AI 能力目标行的拖拽柄启动一次拖拽交互。
    StartAiCapabilityRouteDrag {
        /// Semantic capability owning the route row.
        /// 拥有该路由行的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector that may become the active drag source.
        /// 可能升级为激活拖拽源的完整 `供应商/目标` 选择器。
        target_selector: String,
    },
    /// Updates the drop-preview target while one AI capability route row is being dragged.
    /// 在拖拽某条 AI 能力路由行期间更新插入目标预览。
    AiCapabilityRouteDropPreviewMoved {
        /// Semantic capability owning the route row under the cursor.
        /// 当前光标所在路由行所属的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector represented by the hovered row.
        /// 当前悬停行代表的完整 `供应商/目标` 选择器。
        target_selector: String,
        /// Latest local pointer position inside the hovered row.
        /// 当前悬停行内部最近一次局部鼠标位置。
        position: iced::Point,
    },
    /// Updates the tail drop-preview state while dragging inside one AI capability route list.
    /// 在单个 AI 能力路由列表内部拖拽时更新尾部落点预览状态。
    AiCapabilityRouteTailHovered {
        /// Semantic capability owning the tail drop zone.
        /// 拥有该尾部落点的语义能力。
        capability_kind: AiToolKind,
        /// Whether the cursor is currently hovering the tail drop zone.
        /// 当前鼠标是否正悬停在尾部落点之上。
        is_hovered: bool,
    },
    /// Removes one enabled capability target from its semantic capability page.
    /// 从其语义能力页中移除单条已启用能力目标。
    RemoveAiCapabilityRouteTarget {
        /// Semantic capability owning the route order.
        /// 拥有该路由顺序的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector to remove.
        /// 需要移除的完整 `供应商/目标` 选择器。
        target_selector: String,
    },
    /// Opens the workbench quick AI capability panel from the composer controls.
    /// 从提示词控制区打开工作台快捷 AI 能力面板。
    OpenWorkbenchAiCapabilityPanel,
    /// Closes the workbench quick AI capability panel without changing the selected section.
    /// 在不改变当前分区选择的前提下关闭工作台快捷 AI 能力面板。
    CloseWorkbenchAiCapabilityPanel,
    /// Selects one capability section inside the workbench quick AI capability panel.
    /// 在工作台快捷 AI 能力面板内部选中某个能力分区。
    WorkbenchAiCapabilitySectionSelected(WorkbenchAiCapabilitySection),
    /// Quickly enables or disables one capability from the top checkbox grid of the quick capability panel.
    /// 从快捷能力面板顶部勾选网格中快速启用或关闭某个能力。
    WorkbenchAiCapabilitySectionEnabledToggled {
        /// Quick-panel section whose effective enabled state is being toggled.
        /// 当前被切换生效启用状态的快捷面板分区。
        section: WorkbenchAiCapabilitySection,
        /// Desired enabled state after the toggle interaction.
        /// 当前切换交互结束后期望达到的启用状态。
        enabled: bool,
    },
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
    /// Closes the currently visible MCP server editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的 MCP 服务器编辑器。
    CloseMcpServerEditor,
    /// Updates the editable MCP server name field.
    /// 更新可编辑 MCP 服务器名称字段。
    McpServerNameChanged(String),
    /// Selects the editable MCP transport family.
    /// 选择可编辑 MCP 传输类型。
    McpServerTransportSelected(PersistedMcpTransportKind),
    /// Updates the editable MCP server command field.
    /// 更新可编辑 MCP 服务器命令字段。
    McpServerCommandChanged(String),
    /// Updates one editable MCP argument row.
    /// 更新一条可编辑 MCP 参数行。
    McpServerArgRowChanged(usize, String),
    /// Appends one blank MCP argument row.
    /// 追加一条空白 MCP 参数行。
    AddMcpServerArgRow,
    /// Removes one editable MCP argument row while preserving at least one row.
    /// 删除一条可编辑 MCP 参数行并保证至少保留一行。
    RemoveMcpServerArgRow(usize),
    /// Updates the editable MCP server URL field.
    /// 更新可编辑 MCP 服务器 URL 字段。
    McpServerUrlChanged(String),
    /// Updates the editable MCP server bearer token field.
    /// 更新可编辑 MCP 服务器 Bearer Token 字段。
    McpServerBearerTokenChanged(String),
    /// Updates one editable explicit MCP HTTP-header key row.
    /// 更新一条可编辑显式 MCP HTTP 请求头键行。
    McpServerHeaderKeyChanged(usize, String),
    /// Updates one editable explicit MCP HTTP-header value row.
    /// 更新一条可编辑显式 MCP HTTP 请求头值行。
    McpServerHeaderValueChanged(usize, String),
    /// Appends one blank explicit MCP HTTP-header row.
    /// 追加一条空白显式 MCP HTTP 请求头行。
    AddMcpServerHeaderRow,
    /// Removes one editable explicit MCP HTTP-header row while preserving at least one row.
    /// 删除一条可编辑显式 MCP HTTP 请求头行并保证至少保留一行。
    RemoveMcpServerHeaderRow(usize),
    /// Updates one editable environment-backed MCP HTTP-header key row.
    /// 更新一条可编辑环境变量来源 MCP HTTP 请求头键行。
    McpServerHeaderFromEnvKeyChanged(usize, String),
    /// Updates one editable environment-backed MCP HTTP-header environment-name row.
    /// 更新一条可编辑环境变量来源 MCP HTTP 请求头环境变量名行。
    McpServerHeaderFromEnvValueChanged(usize, String),
    /// Appends one blank environment-backed MCP HTTP-header row.
    /// 追加一条空白环境变量来源 MCP HTTP 请求头行。
    AddMcpServerHeaderFromEnvRow,
    /// Removes one editable environment-backed MCP HTTP-header row while preserving at least one row.
    /// 删除一条可编辑环境变量来源 MCP HTTP 请求头行并保证至少保留一行。
    RemoveMcpServerHeaderFromEnvRow(usize),
    /// Updates the editable MCP server OAuth client identifier field.
    /// 更新可编辑 MCP 服务器 OAuth 客户端标识字段。
    McpServerOAuthClientIdChanged(String),
    /// Updates the editable MCP server OAuth client secret field.
    /// 更新可编辑 MCP 服务器 OAuth 客户端密钥字段。
    McpServerOAuthClientSecretChanged(String),
    /// Updates the editable MCP server OAuth scopes field.
    /// 更新可编辑 MCP 服务器 OAuth scopes 字段。
    McpServerOAuthScopesChanged(String),
    /// Updates the editable MCP server OAuth redirect URI field.
    /// 更新可编辑 MCP 服务器 OAuth 回调 URI 字段。
    McpServerOAuthRedirectUriChanged(String),
    /// Updates the editable MCP server working-directory field.
    /// 更新可编辑 MCP 服务器工作目录字段。
    McpServerCwdChanged(String),
    /// Updates one editable explicit MCP environment-variable key row.
    /// 更新一条可编辑显式 MCP 环境变量键行。
    McpServerEnvKeyChanged(usize, String),
    /// Updates one editable explicit MCP environment-variable value row.
    /// 更新一条可编辑显式 MCP 环境变量值行。
    McpServerEnvValueChanged(usize, String),
    /// Appends one blank explicit MCP environment-variable row.
    /// 追加一条空白显式 MCP 环境变量行。
    AddMcpServerEnvRow,
    /// Removes one editable explicit MCP environment-variable row while preserving at least one row.
    /// 删除一条可编辑显式 MCP 环境变量行并保证至少保留一行。
    RemoveMcpServerEnvRow(usize),
    /// Updates one editable passthrough-environment row.
    /// 更新一条可编辑透传环境变量行。
    McpServerEnvPassthroughChanged(usize, String),
    /// Appends one blank passthrough-environment row.
    /// 追加一条空白透传环境变量行。
    AddMcpServerEnvPassthroughRow,
    /// Removes one editable passthrough-environment row while preserving at least one row.
    /// 删除一条可编辑透传环境变量行并保证至少保留一行。
    RemoveMcpServerEnvPassthroughRow(usize),
    /// Updates the editable MCP OAuth callback input field used for manual authorization completion.
    /// 更新用于手动完成授权的 MCP OAuth 回调输入字段。
    McpServerOAuthCallbackInputChanged(String),
    /// Toggles the editable MCP server enabled flag.
    /// 切换可编辑 MCP 服务器启用标记。
    McpServerEnabledToggled(bool),
    /// Creates a new editable MCP server draft.
    /// 创建一份新的可编辑 MCP 服务器草稿。
    CreateMcpServer,
    /// Persists the currently edited MCP server entry.
    /// 持久化当前编辑中的 MCP 服务器条目。
    SaveMcpServerConfiguration,
    /// Requests removal for one configured MCP server entry from the list page.
    /// 从列表页请求移除一条已配置 MCP 服务器条目。
    RequestRemoveMcpServerConfiguration(String),
    /// Starts the host-driven OAuth authorization flow for the currently selected MCP server.
    /// 为当前选中的 MCP 服务器启动宿主驱动的 OAuth 授权流程。
    StartSelectedMcpServerOAuth,
    /// Applies the OAuth challenge payload generated for one selected MCP server.
    /// 应用为某个选中 MCP 服务器生成的 OAuth 挑战载荷。
    SelectedMcpServerOAuthChallengeReady(String, Result<(String, String), String>),
    /// Applies the browser-open attempt result produced for one MCP OAuth flow.
    /// 应用为一条 MCP OAuth 流程执行的浏览器打开结果。
    McpServerOAuthBrowserOpened(Result<(), String>),
    /// Applies one locally captured OAuth callback payload for the selected MCP server.
    /// 应用为当前选中 MCP 服务器捕获到的一份本地 OAuth 回调载荷。
    SelectedMcpServerOAuthCallbackCaptured(String, Result<String, String>),
    /// Finishes the selected MCP server OAuth flow from the current manual callback input.
    /// 使用当前手动回调输入完成所选 MCP 服务器的 OAuth 流程。
    CompleteSelectedMcpServerOAuthFromInput,
    /// Applies the completion result of one selected MCP server OAuth flow.
    /// 应用某个已选 MCP 服务器 OAuth 流程的完成结果。
    SelectedMcpServerOAuthFinished(String, Result<String, String>),
    /// Removes stored OAuth credentials for the currently selected MCP server.
    /// 移除当前选中 MCP 服务器的已存储 OAuth 凭据。
    RemoveSelectedMcpServerOAuthAuthorization,
    /// Applies the removal result of one selected MCP server OAuth credential set.
    /// 应用某个已选 MCP 服务器 OAuth 凭据集合的移除结果。
    SelectedMcpServerOAuthAuthorizationRemoved(String, Result<(), String>),
    /// Applies the latest OAuth status summary loaded for one selected MCP server.
    /// 应用为某个选中 MCP 服务器载入的最新 OAuth 状态摘要。
    SelectedMcpServerOAuthStatusLoaded(String, Result<String, String>),
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
    /// Opens the project settings modal for one task-tree project.
    /// 为任务树中的某个项目打开项目设置弹窗。
    OpenTaskProjectSettings(String),
    /// Closes the project settings modal and its transient overlays.
    /// 关闭项目设置弹窗及其临时浮层。
    CloseTaskProjectSettings,
    /// Opens the project attachment cleanup cutoff date picker.
    /// 打开项目附件清理截止日期选择器。
    OpenProjectAttachmentCleanupDatePicker,
    /// Cancels the project attachment cleanup cutoff date picker without changing the date.
    /// 取消项目附件清理截止日期选择器且不修改日期。
    CancelProjectAttachmentCleanupDatePicker,
    /// Applies one date selected by the project attachment cleanup date picker.
    /// 应用项目附件清理日期选择器选中的日期。
    ProjectAttachmentCleanupDateSelected(Date),
    /// Toggles whether one attachment kind should participate in project-scoped cleanup preview and execution.
    /// 切换某种附件类型是否参与项目级清理预览与执行。
    ProjectAttachmentCleanupKindToggled(ConversationAttachmentKind, bool),
    /// Builds one preview for the current project-scoped attachment cleanup selection.
    /// 为当前项目级附件清理选择构建一份预览。
    PreviewProjectAttachmentCleanup,
    /// Executes one destructive project-scoped attachment cleanup using the current selection.
    /// 使用当前选择执行一次破坏性的项目级附件清理。
    ExecuteProjectAttachmentCleanup,
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
    WorkbenchReasoningEffortSelected(ReasoningSelectionOption),
    /// Opens the compact reasoning-effort picker modal from the composer footer trigger.
    /// 从提示词底部触发器打开紧凑思考级别选择弹窗。
    OpenWorkbenchReasoningPicker,
    /// Closes the compact reasoning-effort picker modal.
    /// 关闭紧凑思考级别选择弹窗。
    CloseWorkbenchReasoningPicker,
    /// Records the current viewport of the virtualized workbench model-picker list.
    /// 记录虚拟化工作台模型选择列表的当前视口状态。
    WorkbenchModelPickerScrolled(scrollable::Viewport),
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
    /// Opens one cached subagent task panel inside the main transcript region.
    /// 在主转录区域内部打开一个已缓存的子代理任务面板。
    OpenSelectedSessionSubagentPanel(SessionId),
    /// Updates whether the left-edge subagent navigation panel is expanded by pointer hover.
    /// 更新左侧贴边子代理导航面板是否因鼠标悬停而展开。
    SubagentNavigationHoverChanged(bool),
    /// Stops the currently opened running subagent task without aborting the parent session.
    /// 停止当前已打开的运行中子代理任务，而不影响父会话。
    StopSelectedSubagentTask,
    /// Returns from the currently opened subagent task panel back to the parent session transcript.
    /// 从当前已打开的子代理任务面板返回父会话转录视图。
    ReturnToSelectedSessionTranscript,
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
    /// Requests loading one older transcript page for the visible root session.
    /// 请求为当前可见根会话加载一页更早转录历史。
    LoadOlderSessionHistoryRequested(SessionId),
    /// Opens the dedicated workspace-history search modal from the left task drawer trigger.
    /// 通过左侧任务抽屉触发器打开专用工作台历史搜索弹窗。
    OpenWorkspaceHistorySearchPanel,
    /// Closes the dedicated workspace-history search modal without clearing the current query state.
    /// 在不清空当前查询状态的前提下关闭专用工作台历史搜索弹窗。
    CloseWorkspaceHistorySearchPanel,
    /// Updates the current left-panel workspace-history query text.
    /// 更新左侧工作台历史查询文本。
    WorkspaceHistorySearchQueryChanged(String),
    /// Requests running one workspace-scoped BM25 history search.
    /// 请求执行一次工作区级 BM25 历史搜索。
    SubmitWorkspaceHistorySearch,
    /// Requests loading the next page of the current workspace-history search result set.
    /// 请求继续加载当前工作台历史搜索结果的下一页。
    LoadMoreWorkspaceHistorySearchResults,
    /// Applies the completed workspace-scoped BM25 history search result page.
    /// 应用已完成的工作区级 BM25 历史搜索结果页。
    WorkspaceHistorySearchLoaded {
        /// Whether the loaded page should be appended to the current result set.
        /// 本次加载完成的结果页是否应追加到当前结果集中。
        append: bool,
        /// Final page payload or the failure returned by the controller-backed search task.
        /// 控制器承载搜索任务返回的最终结果页或失败信息。
        result: Result<WorkspaceHistorySearchPage, String>,
    },
    /// Toggles the project-scope dropdown used by the left search panel.
    /// 切换左侧搜索面板使用的项目范围下拉区。
    ToggleWorkspaceHistorySearchProjectPicker,
    /// Updates the project keyword filter shown inside the left search panel scope dropdown.
    /// 更新左侧搜索面板范围下拉区中的项目关键字过滤文本。
    WorkspaceHistorySearchProjectFilterChanged(String),
    /// Applies one concrete workspace-history search scope chosen from the left dropdown.
    /// 应用从左侧下拉区选择的一条具体工作台历史搜索范围。
    WorkspaceHistorySearchScopeSelected(WorkspaceHistorySearchScope),
    /// Confirms the currently highlighted project-scope filter selection.
    /// 确认当前项目范围过滤列表中的高亮选择。
    ConfirmWorkspaceHistorySearchScopeSelection,
    /// Opens one workspace-history search hit and navigates the transcript to the matched message.
    /// 打开一条工作台历史搜索命中并将转录定位到匹配消息。
    OpenWorkspaceHistorySearchHit {
        /// Stable session identifier owning the matched message row.
        /// 拥有该命中消息行的稳定会话标识。
        session_id: SessionId,
        /// Stable message identifier that should become visible in the transcript.
        /// 应在转录中变为可见的稳定消息标识。
        message_id: String,
        /// Stable ordinal used to decide whether older history still needs loading.
        /// 用于判断是否仍需补载更早历史的稳定序号。
        ordinal: i64,
        /// Stable turn index used for user-facing status copy.
        /// 用于面向用户状态文案的稳定轮次索引。
        turn_index: i64,
    },
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
    /// Drives one coalesced tail-follow tick for the main rendered transcript surface.
    /// 驱动主渲染转录表面的一次合并尾部追随 Tick。
    SelectedSessionTranscriptFollowTailTick,
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
    /// Drives one coalesced tail-follow tick for the floating thinking overlay body.
    /// 驱动流式思考浮层正文的一次合并尾部追随 Tick。
    SelectedSessionThinkingOverlayFollowTailTick,
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
    /// Dispatches one existing queued prompt immediately, interrupting the current run first when needed.
    /// 立即分发一条现有排队提示词；若当前仍在运行则先中断当前执行。
    PendingPromptQueueInsertNow(String),
    /// Restores one queued prompt back into the composer editor for further editing.
    /// 将某条已排队提示词恢复回提示词编辑器，以便继续修改。
    PendingPromptQueueEdit(String),
    /// Removes one queued prompt from the selected session queue without dispatching it.
    /// 从当前选中会话队列中移除某条已排队提示词且不再发送。
    PendingPromptQueueRemove(String),
    /// Starts one click-or-drag interaction from a pending-prompt queue handle.
    /// 从某条待发送队列拖拽柄启动一次点击或拖拽交互。
    StartPendingPromptQueueDrag(String),
    /// Updates the drop-target preview inside one pending-prompt queue row using local pointer movement.
    /// 使用队列行内部的局部鼠标移动更新待发送队列的插入目标预览。
    PendingPromptQueueDropPreviewMoved(String, iced::Point),
    /// Updates whether the tail drop zone of the pending-prompt queue is currently hovered.
    /// 更新待发送队列尾部落点当前是否处于悬停状态。
    PendingPromptQueueTailHovered(bool),
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
    /// Applies one detached background subagent event emitted after the parent run has already resumed or finished.
    /// 应用一条在父运行恢复或结束之后发出的分离后台子代理事件。
    DetachedSubagentTaskEvent {
        /// Stable detached child-task identifier that owns the listener stream.
        /// 拥有该监听流的稳定分离子任务标识。
        task_id: SessionId,
        /// Concrete detached background event published by the task runtime.
        /// 由任务运行时发布的具体分离后台事件。
        event: DetachedTaskRuntimeEvent,
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
        let (
            ai_tool_configuration,
            ai_tool_notice,
            ai_tool_configuration_migrated,
            ai_tool_configuration_normalized,
        ) = match ai_tool_configuration::load_ai_tool_configuration_file(&provider_configuration) {
            Ok(loaded) => match loaded.source {
                AiToolConfigurationLoadSource::DedicatedFile => {
                    let mut notice = String::from(
                        "已从 `~/.vulcan/vulcan-ai-capabilities.json` 载入独立 AI 能力配置。",
                    );
                    if loaded.normalized {
                        match ai_tool_configuration::save_ai_tool_configuration_file(
                            &loaded.configuration,
                        ) {
                            Ok(()) => notice.push_str(" 已自动归一化旧版重复 AI 能力提供方配置。"),
                            Err(error) => notice.push_str(&format!(
                                " 但自动归一化旧版重复 AI 能力提供方配置时写回失败：{error}"
                            )),
                        }
                    }
                    (loaded.configuration, notice, false, loaded.normalized)
                }
                AiToolConfigurationLoadSource::LegacyProviderFile => {
                    match ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    ) {
                        Ok(()) => {
                            provider_configuration.ai_tools =
                                PersistedAiToolConfiguration::default();
                            provider_notice
                                .push_str(" 已将旧版内嵌 AI 能力配置迁移到独立配置文件。");
                            (
                                loaded.configuration,
                                String::from(
                                    "已将旧版供应商内嵌 AI 能力配置迁移到 `~/.vulcan/vulcan-ai-capabilities.json`。",
                                ),
                                true,
                                loaded.normalized,
                            )
                        }
                        Err(error) => (
                            loaded.configuration,
                            format!("独立 AI 能力配置迁移失败，当前仍使用旧版内嵌配置：{error}"),
                            false,
                            loaded.normalized,
                        ),
                    }
                }
                AiToolConfigurationLoadSource::Default => (
                    loaded.configuration,
                    String::from("尚未检测到独立 AI 能力配置，当前使用默认能力层设置。"),
                    false,
                    false,
                ),
            },
            Err(error) => {
                if provider_configuration.ai_tools.is_default() {
                    (
                        PersistedAiToolConfiguration::default(),
                        format!("独立 AI 能力配置载入失败，当前回退到默认能力层设置：{error}"),
                        false,
                        false,
                    )
                } else {
                    (
                        provider_configuration.ai_tools.clone(),
                        format!("独立 AI 能力配置载入失败，当前回退到旧版内嵌能力层配置：{error}"),
                        false,
                        false,
                    )
                }
            }
        };
        if runtime_defaults_changed
            || legacy_agent_migrated
            || ai_tool_configuration_migrated
            || ai_tool_configuration_normalized
        {
            match provider_config::save_configuration_file(&provider_configuration) {
                Ok(()) => {
                    if runtime_defaults_changed {
                        provider_notice
                            .push_str(" 已自动补齐运行时/compose/mcp 基础架构默认配置。");
                    }
                    if legacy_agent_migrated {
                        provider_notice.push_str(" 旧版 Provider Agent 字段已从主配置文件中清退。");
                    }
                    if ai_tool_configuration_migrated {
                        provider_notice
                            .push_str(" 旧版 Provider AI 能力字段已从主配置文件中清退。");
                    }
                    if ai_tool_configuration_normalized {
                        provider_notice
                            .push_str(" 独立 AI 能力配置中的重复提供方条目已被自动归一化。");
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
        let initial_workbench_reasoning_effort =
            Self::resolve_initial_workbench_reasoning_effort(&provider_configuration);
        let runtime_foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &provider_configuration,
            &agent_profile_catalog,
            None,
            None,
        );

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
        let initial_execution_mode = if matches!(
            agent_profile_catalog.state.selected_agent_id.as_deref(),
            Some(agent_id) if agent_id == PLAN_AGENT_ID || agent_id == EXPLORE_AGENT_ID
        ) {
            ExecutionMode::Plan
        } else {
            ExecutionMode::Build
        };
        let initial_workbench_disabled_ai_capability_kinds =
            ai_tool_configuration.disabled_capability_kinds.clone();

        let mut application = Self {
            main_window_id: Some(main_window_id),
            diff_window_id: None,
            main_window_size: main_window_settings().size,
            main_window_maximized: main_window_settings().maximized,
            diff_window_size: diff_window_settings().size,
            diff_window_maximized: diff_window_settings().maximized,
            main_window_cursor_position: None,
            workbench_keyboard_modifiers: keyboard::Modifiers::default(),
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
            composer_attachments: Vec::new(),
            workbench_notice_modal: None,
            workbench_image_preview_modal: None,
            generated_audio_playback: None,
            composer_drag_hover_labels: Vec::new(),
            composer_undo_history: Vec::new(),
            composer_scroll_offset: 0.0,
            composer_scrollbar_pointer_y: None,
            composer_scrollbar_drag_grab_offset: None,
            workbench_context_menu: None,
            composer_context_clipboard_text: None,
            selectable_transcript_command: None,
            selectable_transcript_command_revision: 0,
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
            active_pending_prompt_drop_target: None,
            pending_pending_prompt_drag: None,
            dragging_pending_prompt_entry_id: None,
            active_ai_capability_route_drop_target: None,
            pending_ai_capability_route_drag: None,
            dragging_ai_capability_route_target: None,
            selected_session: SessionId::nil(),
            activities,
            activity_raw_buffers,
            session_messages: BTreeMap::new(),
            session_tool_detail_messages: BTreeMap::new(),
            archived_sessions: Vec::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            hydrated_session_ids: BTreeSet::new(),
            dirty_session_payload_ids: BTreeSet::new(),
            dirty_session_message_ids: BTreeSet::new(),
            loading_session_ids: BTreeSet::new(),
            loading_session_agent_view_projection_ids: BTreeSet::new(),
            session_history_earliest_ordinals: BTreeMap::new(),
            session_history_has_more: BTreeSet::new(),
            loading_older_session_history_ids: BTreeSet::new(),
            workspace_history_search: WorkspaceHistorySearchPanelState::default(),
            background_legacy_maintenance_queue: VecDeque::new(),
            background_legacy_maintenance_in_flight: None,
            background_legacy_maintenance_completed_count: 0,
            background_legacy_maintenance_failed_count: 0,
            workspace_snapshot_persist_in_flight: false,
            workspace_snapshot_persist_pending: false,
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
            session_subagent_tasks: BTreeMap::new(),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: None,
            subagent_navigation_expanded: false,
            detached_subagent_task_listeners: BTreeSet::new(),
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
            session_inline_agent_view_documents: BTreeMap::new(),
            session_agent_view_block_collapsed: BTreeMap::new(),
            session_agent_view_follow_tail: BTreeMap::new(),
            session_agent_view_viewports: BTreeMap::new(),
            pending_transcript_follow_tail_session_ids: BTreeSet::new(),
            active_transcript_drag_auto_scroll: None,
            session_streaming_thinking_viewports: BTreeMap::new(),
            pending_thinking_overlay_follow_tail_session_ids: BTreeSet::new(),
            active_thinking_overlay_drag_auto_scroll: None,
            agent_transcript_surface_mode: AgentTranscriptSurfaceMode::DetailCards,
            session_run_phases: BTreeMap::new(),
            active_runs: BTreeMap::new(),
            next_run_id: 1,
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
            ai_tool_configuration,
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
            selected_settings_section: SettingsSection::ModelProviders,
            settings_ai_capability_tree_expanded: true,
            settings_model_tree_expanded: true,
            ai_capability_vendor_picker: None,
            ai_capability_vendor_editor: None,
            ai_capability_target_picker: None,
            ai_capability_routing_picker: None,
            provider_editor: None,
            provider_model_picker: None,
            provider_model_override_editor: None,
            selected_provider,
            provider_form,
            provider_notice,
            ai_tool_notice,
            mcp_notice: if selected_mcp_server_id.is_some() {
                String::from("已载入 MCP 服务器配置。")
            } else {
                String::from("尚未配置 MCP 服务器，可在此新增一个 stdio 或 http 服务器。")
            },
            selected_mcp_server_id,
            mcp_server_editor_open: false,
            mcp_server_form,
            mcp_oauth_status: String::from("未配置 OAuth"),
            mcp_oauth_callback_input: String::new(),
            mcp_oauth_authorization_url: String::new(),
            mcp_oauth_pending_state: None,
            mcp_oauth_flow_in_flight: false,
            project_settings_project_id: None,
            project_attachment_cleanup_cutoff_date: Date::today(),
            project_attachment_cleanup_date_picker_open: false,
            project_attachment_cleanup_selection: AttachmentCleanupSelection::all_enabled(),
            project_attachment_cleanup_preview: None,
            project_attachment_cleanup_last_report: None,
            project_attachment_cleanup_notice: String::from(
                "选择截止日期后可先预览当前项目将被清理的历史附件缓存，再执行实际清理。",
            ),
            last_execution_preparation: None,
            last_mcp_discovery_rows: Vec::new(),
            last_mcp_discovery_errors: Vec::new(),
            execution_mode: initial_execution_mode,
            workbench_reasoning_effort_preference: initial_workbench_reasoning_effort,
            workbench_reasoning_effort: initial_workbench_reasoning_effort,
            workbench_web_search_override: PendingPromptWebSearchOverride::FollowGlobal,
            workbench_web_search_provider_id: None,
            workbench_disabled_ai_capability_kinds: initial_workbench_disabled_ai_capability_kinds,
            workbench_ai_capability_panel_open: false,
            workbench_ai_capability_section: WorkbenchAiCapabilitySection::WebSearch,
            workbench_model_picker_open: false,
            workbench_reasoning_picker_open: false,
            workbench_model_picker_filter: String::new(),
            workbench_model_picker_highlighted_index: None,
            workbench_model_picker_scroll_offset: 0.0,
            workbench_model_picker_viewport_height: WORKBENCH_MODEL_PICKER_LIST_HEIGHT,
            app_theme,
            temperature_enabled: false,
            temperature: 0.25,
            context_usage: 0.0,
            context_usage_detail: String::from("尚未生成预算快照"),
            generate_plan: false,
            run_cargo_check: true,
            persist_memory: true,
            auto_approve: false,
            status_message: String::from("正在初始化 vldb 数据平面。"),
        };

        let _ = application.reconcile_workbench_reasoning_effort_for_active_model();

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

    /// Returns the current draggable scrollbar layout for the capped composer editor, when scrolling is actually needed.
    /// 返回当前封顶提示词编辑器在真正需要滚动时的可拖拽滚动条布局信息。
    fn composer_scrollbar_layout(&self) -> Option<ComposerScrollbarLayout> {
        let (content_height, viewport_height, scroll_offset) =
            self.composer_editor_scroll_metrics();
        let max_scroll_offset = (content_height - viewport_height).max(0.0);

        if max_scroll_offset <= 0.5 {
            return None;
        }

        let track_height = (self.composer_editor_height() - 16.0).max(0.0);
        let thumb_height =
            ((viewport_height / content_height) * track_height).clamp(24.0, track_height);
        let thumb_travel = (track_height - thumb_height).max(0.0);
        let thumb_top = if thumb_travel > 0.0 {
            (scroll_offset / max_scroll_offset).clamp(0.0, 1.0) * thumb_travel
        } else {
            0.0
        };

        Some(ComposerScrollbarLayout {
            track_height,
            thumb_height,
            thumb_travel,
            thumb_top,
            max_scroll_offset,
        })
    }

    /// Scrolls the composer editor according to one local pointer y position measured inside the editor surface.
    /// 根据在编辑器表面内部测得的一次局部纵向指针位置滚动提示词编辑器。
    fn apply_composer_scrollbar_pointer_position(&mut self, pointer_y: f32) {
        let Some(layout) = self.composer_scrollbar_layout() else {
            self.composer_scrollbar_drag_grab_offset = None;
            return;
        };
        let Some(grab_offset) = self.composer_scrollbar_drag_grab_offset else {
            return;
        };

        let local_track_y = (pointer_y - 8.0).clamp(0.0, layout.track_height);
        let thumb_top = (local_track_y - grab_offset).clamp(0.0, layout.thumb_travel);
        let target_scroll_offset = if layout.thumb_travel > 0.0 {
            (thumb_top / layout.thumb_travel) * layout.max_scroll_offset
        } else {
            0.0
        };

        self.composer_buffer
            .scroll_to_plain_text_offset(target_scroll_offset);

        let (_, actual_scroll_offset) = self.composer_buffer.plain_text_scroll_metrics();
        self.composer_scroll_offset = actual_scroll_offset.clamp(0.0, layout.max_scroll_offset);
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
                if let Some(runtime_access) = self.mcp_runtime.take() {
                    runtime_access.shutdown();
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
                self.session_tool_detail_messages.clear();
                self.archived_sessions.clear();
                self.archived_session_messages.clear();
                self.session_compactions.clear();
                self.archived_session_compactions.clear();
                self.session_runtime_state.clear();
                self.archived_session_runtime_state.clear();
                self.hydrated_session_ids.clear();
                self.dirty_session_payload_ids.clear();
                self.dirty_session_message_ids.clear();
                self.loading_session_ids.clear();
                self.session_history_earliest_ordinals.clear();
                self.session_history_has_more.clear();
                self.loading_older_session_history_ids.clear();
                self.workspace_history_search = WorkspaceHistorySearchPanelState::default();
                self.background_legacy_maintenance_queue.clear();
                self.background_legacy_maintenance_in_flight = None;
                self.background_legacy_maintenance_completed_count = 0;
                self.background_legacy_maintenance_failed_count = 0;
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
                self.session_subagent_tasks.clear();
                self.session_subagent_navigation_rounds.clear();
                self.active_subagent_panel = None;
                self.subagent_navigation_expanded = false;
                self.detached_subagent_task_listeners.clear();
                self.streaming_output_message_ids.clear();
                self.runtime_event_raw_buffers.clear();
                self.runtime_event_markdown.clear();
                self.runtime_event_markdown_buffers.clear();
                self.session_agent_view_buffers.clear();
                self.session_agent_view_raw_buffers.clear();
                self.session_last_api_request_buffers.clear();
                self.session_agent_view_selectable_documents.clear();
                self.session_agent_view_documents.clear();
                self.session_inline_agent_view_documents.clear();
                self.session_agent_view_block_collapsed.clear();
                self.session_agent_view_follow_tail.clear();
                self.session_agent_view_viewports.clear();
                self.pending_transcript_follow_tail_session_ids.clear();
                self.session_streaming_thinking_viewports.clear();
                self.pending_thinking_overlay_follow_tail_session_ids
                    .clear();
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
                                    bootstrap_mcp_runtime_access(
                                        self.provider_configuration.clone(),
                                    ),
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
                        self.status_message =
                            String::from("工作台列表已加载完成，当前会话正文将按需载入。");
                        self.push_activity(
                            ActivityLevel::Success,
                            "工作台数据已加载",
                            format!(
                                "已恢复 {} 个项目与 {} 个会话；会话正文按需加载，避免启动阶段拉取超大历史。",
                                self.task_projects.len(),
                                self.sessions.len()
                            ),
                        );
                        let hydration_task = self
                            .selected_session()
                            .map(|session| session.id)
                            .map(|session_id| self.schedule_session_hydration(session_id))
                            .unwrap_or_else(Task::none);
                        let legacy_maintenance_task =
                            self.schedule_next_background_legacy_session_maintenance();
                        return Task::batch([
                            self.schedule_project_temp_cleanup_for_all_projects(),
                            hydration_task,
                            legacy_maintenance_task,
                        ]);
                    }
                    Err(error) => {
                        self.status_message = format!("工作台数据加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台数据加载失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionHydrationLoaded(session_id, result) => {
                self.loading_session_ids.remove(&session_id);
                match result {
                    Ok(snapshot) => {
                        let tool_detail_turn_indices = snapshot.tool_detail_turn_indices.clone();
                        self.apply_session_hydration_snapshot(snapshot);
                        let maintenance_task =
                            self.schedule_session_storage_maintenance(session_id);
                        let projection_task = self
                            .schedule_session_agent_view_projection_refresh_if_needed(session_id);
                        if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            let continue_search_navigation =
                                self.maybe_continue_workspace_history_search_navigation(session_id);
                            self.status_message = String::from("当前会话正文已按需加载完成。");
                            return Task::batch([
                                maintenance_task,
                                projection_task,
                                self.schedule_session_tool_detail_loads(
                                    session_id,
                                    &tool_detail_turn_indices,
                                ),
                                self.maybe_follow_selected_session_transcript_tail(session_id),
                                self.maybe_follow_selected_session_thinking_overlay_tail(
                                    session_id,
                                ),
                                continue_search_navigation,
                            ]);
                        }

                        return Task::batch([maintenance_task, projection_task]);
                    }
                    Err(error) => {
                        self.status_message = format!("当前会话正文加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "当前会话正文加载失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionAgentViewProjectionLoaded(session_id, result) => {
                if !self
                    .loading_session_agent_view_projection_ids
                    .remove(&session_id)
                {
                    return Task::none();
                }

                match result {
                    Ok(snapshot) => {
                        self.apply_session_agent_view_projection_snapshot(snapshot);
                        if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            return Task::batch([
                                self.maybe_follow_selected_session_transcript_tail(session_id),
                                self.maybe_follow_selected_session_thinking_overlay_tail(
                                    session_id,
                                ),
                            ]);
                        }
                    }
                    Err(error) => {
                        self.status_message = format!("当前会话视图构建失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "当前会话视图构建失败", error);
                    }
                }

                Task::none()
            }
            Message::OlderSessionHistoryLoaded(session_id, result) => {
                self.loading_older_session_history_ids.remove(&session_id);
                match result {
                    Ok(page) => {
                        let tool_detail_turn_indices = page.tool_detail_turn_indices.clone();
                        self.apply_older_session_history_page(page);
                        self.status_message = String::from("已加载更早一段会话历史。");
                        return Task::batch([
                            self.schedule_session_tool_detail_loads(
                                session_id,
                                &tool_detail_turn_indices,
                            ),
                            self.maybe_continue_workspace_history_search_navigation(session_id),
                        ]);
                    }
                    Err(error) => {
                        self.status_message = format!("加载更早会话历史失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "加载更早会话历史失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionToolDetailsLoaded(session_id, result) => {
                match result {
                    Ok(page) => {
                        self.apply_session_tool_detail_page(page);
                        let maintenance_task =
                            self.schedule_session_storage_maintenance(session_id);
                        let follow_task = if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            self.maybe_follow_selected_session_transcript_tail(session_id)
                        } else {
                            Task::none()
                        };
                        return Task::batch([maintenance_task, follow_task]);
                    }
                    Err(error) => {
                        self.push_activity(ActivityLevel::Warning, "工具明细加载失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionMaintenanceFinished(session_id, result) => {
                match result {
                    Ok(report) => {
                        if report.session_id != session_id || !report.cleared_runtime_state_json {
                            self.push_activity(
                                ActivityLevel::Warning,
                                "会话旧数据维护结果异常",
                                format!(
                                    "维护报告与请求会话不一致或旧状态清理未确认，message_count={}",
                                    report.upserted_message_count
                                ),
                            );
                        }
                        self.record_session_legacy_chunk_migration_activity(
                            "旧版超大消息已迁移",
                            &report,
                        );
                    }
                    Err(error) => {
                        self.push_activity(ActivityLevel::Warning, "会话旧数据维护失败", error);
                    }
                }

                Task::none()
            }
            Message::BackgroundLegacySessionMaintenanceFinished(session_id, result) => {
                if self.background_legacy_maintenance_in_flight == Some(session_id) {
                    self.background_legacy_maintenance_in_flight = None;
                }
                match result {
                    Ok(report) => {
                        self.background_legacy_maintenance_completed_count = self
                            .background_legacy_maintenance_completed_count
                            .saturating_add(1);
                        self.record_session_legacy_chunk_migration_activity(
                            "后台旧版超大消息已迁移",
                            &report,
                        );
                    }
                    Err(error) => {
                        self.background_legacy_maintenance_failed_count = self
                            .background_legacy_maintenance_failed_count
                            .saturating_add(1);
                        self.push_activity(
                            ActivityLevel::Warning,
                            "后台旧会话维护失败",
                            format!("session_id={session_id}，{error}"),
                        );
                    }
                }

                let next_task = self.schedule_next_background_legacy_session_maintenance();
                if self.background_legacy_maintenance_in_flight.is_none()
                    && self.background_legacy_maintenance_queue.is_empty()
                    && (self.background_legacy_maintenance_completed_count > 0
                        || self.background_legacy_maintenance_failed_count > 0)
                {
                    let level = if self.background_legacy_maintenance_failed_count == 0 {
                        ActivityLevel::Success
                    } else {
                        ActivityLevel::Warning
                    };
                    self.push_activity(
                        level,
                        "后台旧会话维护完成",
                        format!(
                            "已处理 {} 个会话，失败 {} 个。后台维护只执行轻量迁移与旧状态清理，未水合完整正文。",
                            self.background_legacy_maintenance_completed_count,
                            self.background_legacy_maintenance_failed_count
                        ),
                    );
                }

                next_task
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
                self.workspace_snapshot_persist_in_flight = false;
                let next_persist_task = if self.workspace_snapshot_persist_pending {
                    self.workspace_snapshot_persist_pending = false;
                    self.persist_workspace_snapshot_task()
                } else {
                    Task::none()
                };
                if let Err(error) = result {
                    self.status_message = format!("工作台状态保存失败：{error}");
                    self.push_activity(ActivityLevel::Warning, "工作台状态保存失败", error);
                }

                next_persist_task
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
                    Ok(runtime_access) => {
                        self.mcp_runtime = Some(runtime_access);
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

                self.load_selected_mcp_server_oauth_status_task()
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
                self.refresh_visible_diff_buffers();
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
                self.composer_drag_hover_labels.clear();
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
                        String::from("已切换到设置面板，当前聚焦模型与能力配置。")
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
                    self.refresh_visible_diff_buffers();
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
                    self.dismiss_workbench_context_menu();
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_size = size;
                }

                if self.main_window_id == Some(window_id) || self.diff_window_id == Some(window_id)
                {
                    self.refresh_visible_diff_buffers();
                }

                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::WindowPointerMoved(window_id, position) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_cursor_position = Some(position);
                    if self
                        .workbench_image_preview_modal
                        .as_ref()
                        .is_some_and(|modal| modal.pan_pointer.is_some())
                    {
                        return self.apply_workbench_image_preview_pan(position);
                    }

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
                    if let Some(pending_queue_drag) = self.pending_pending_prompt_drag.as_ref() {
                        if self.dragging_pending_prompt_entry_id.is_none()
                            && ((position.x - pending_queue_drag.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_queue_drag.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_pending_prompt_entry_id =
                                Some(pending_queue_drag.entry_id.clone());
                            self.active_pending_prompt_drop_target =
                                Some(PendingPromptDropTarget::Before(
                                    pending_queue_drag.entry_id.clone(),
                                ));
                        }
                    }
                    if let Some(pending_route_drag) = self.pending_ai_capability_route_drag.as_ref()
                    {
                        if self.dragging_ai_capability_route_target.is_none()
                            && ((position.x - pending_route_drag.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_route_drag.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_ai_capability_route_target =
                                Some(pending_route_drag.target_selector.clone());
                            self.active_ai_capability_route_drop_target =
                                Some(AiCapabilityRouteDropTarget::Before(
                                    pending_route_drag.target_selector.clone(),
                                ));
                        }
                    }
                }
                Task::none()
            }
            Message::WorkbenchKeyboardModifiersChanged(modifiers) => {
                self.workbench_keyboard_modifiers = modifiers;
                Task::none()
            }
            Message::WindowLeftButtonReleased(window_id) => {
                self.clear_transcript_drag_auto_scrolls();
                self.composer_scrollbar_drag_grab_offset = None;
                let mut did_commit_drop = false;
                let mut did_commit_pending_prompt_drop = false;
                if self.main_window_id == Some(window_id) {
                    if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                        modal.pan_pointer = None;
                    }
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

                    let committed_pending_prompt_drop = self
                        .selected_session()
                        .map(|session| session.id)
                        .zip(self.dragging_pending_prompt_entry_id.clone())
                        .zip(self.active_pending_prompt_drop_target.clone())
                        .map(|((session_id, dragged_entry_id), drop_target)| {
                            (session_id, dragged_entry_id, drop_target)
                        });
                    self.pending_pending_prompt_drag = None;
                    if let Some((session_id, dragged_entry_id, drop_target)) =
                        committed_pending_prompt_drop
                    {
                        did_commit_pending_prompt_drop = self.apply_pending_prompt_drop_target(
                            session_id,
                            &dragged_entry_id,
                            &drop_target,
                        );
                    }
                    self.dragging_pending_prompt_entry_id = None;
                    self.active_pending_prompt_drop_target = None;

                    let committed_ai_capability_route_drop = self
                        .pending_ai_capability_route_drag
                        .as_ref()
                        .map(|drag| drag.capability_kind)
                        .zip(self.dragging_ai_capability_route_target.clone())
                        .zip(self.active_ai_capability_route_drop_target.clone())
                        .map(
                            |((capability_kind, dragged_target_selector), drop_target)| {
                                (capability_kind, dragged_target_selector, drop_target)
                            },
                        );
                    self.pending_ai_capability_route_drag = None;
                    if let Some((capability_kind, dragged_target_selector, drop_target)) =
                        committed_ai_capability_route_drop
                    {
                        let _ = self.apply_ai_capability_route_drop_target(
                            capability_kind,
                            &dragged_target_selector,
                            &drop_target,
                        );
                    }
                    self.dragging_ai_capability_route_target = None;
                    self.active_ai_capability_route_drop_target = None;

                    if let Some(project_id) = toggled_project_id {
                        self.toggle_task_project_expanded(&project_id);
                    }
                }
                if did_commit_drop || did_commit_pending_prompt_drop {
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
            Message::ComposerEditorAction(action) => self.apply_composer_editor_action(action),
            Message::ComposerUndo => self.restore_latest_composer_undo_snapshot(),
            Message::AddComposerAttachments => self.add_composer_attachments(),
            Message::PasteComposerClipboardContents => self.paste_composer_clipboard(false),
            Message::PasteComposerClipboardAttachments => self.paste_composer_clipboard(true),
            Message::RemoveComposerAttachment(local_path) => {
                self.remove_composer_attachment(&local_path)
            }
            Message::OpenComposerImageAttachmentPreview {
                file_name,
                local_path,
            } => self.open_composer_image_attachment_preview(file_name, local_path),
            Message::OpenGeneratedAttachmentWithDefaultApp(local_path) => {
                self.open_generated_attachment_with_system_default_application(&local_path)
            }
            Message::RevealGeneratedAttachmentInFolder(local_path) => {
                match reveal_path_in_system_file_manager(&local_path) {
                    Ok(()) => {
                        self.status_message = String::from("已在文件管理器中定位生成媒体。");
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal("无法定位生成媒体", error.clone());
                        self.status_message = format!("定位生成媒体失败：{error}");
                    }
                }
                Task::none()
            }
            Message::CopyGeneratedAttachmentPath(local_path) => {
                self.status_message = String::from("已复制生成媒体路径。");
                clipboard::write(local_path).discard()
            }
            Message::ToggleGeneratedAudioPlayback {
                file_name,
                local_path,
            } => self.toggle_generated_audio_playback(file_name, local_path),
            Message::CopyGeneratedImageToClipboard {
                file_name,
                local_path,
            } => self.copy_generated_image_to_clipboard(file_name, local_path),
            Message::CloseWorkbenchImagePreviewModal => {
                self.workbench_image_preview_modal = None;
                Task::none()
            }
            Message::AdjustWorkbenchImagePreviewZoom(delta) => {
                self.adjust_workbench_image_preview_zoom(delta)
            }
            Message::WorkbenchImagePreviewWheelScrolled(delta) => {
                self.handle_workbench_image_preview_wheel(delta)
            }
            Message::WorkbenchImagePreviewLeftPressed => self.begin_workbench_image_preview_pan(),
            Message::WorkbenchImagePreviewLeftReleased => self.end_workbench_image_preview_pan(),
            Message::WorkbenchImagePreviewPointerMoved(position) => {
                if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                    modal.preview_pointer = Some(position);
                }
                Task::none()
            }
            Message::WorkbenchImagePreviewScrolled(viewport) => {
                if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                    modal.scroll_offset = viewport.absolute_offset();
                }
                Task::none()
            }
            Message::ResetWorkbenchImagePreviewZoom => self.reset_workbench_image_preview_zoom(),
            Message::ConfirmWorkbenchNoticeModal => self.confirm_workbench_notice_modal(),
            Message::CloseWorkbenchNoticeModal => {
                self.workbench_notice_modal = None;
                Task::none()
            }
            Message::DismissTopLevelOverlay => {
                if self.workbench_context_menu.take().is_some() {
                    self.composer_context_clipboard_text = None;
                    return Task::none();
                }
                if self.workbench_image_preview_modal.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_vendor_editor.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_vendor_picker.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_target_picker.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_routing_picker.take().is_some() {
                    return Task::none();
                }
                if self.provider_model_override_editor.take().is_some() {
                    return Task::none();
                }
                if self.provider_model_picker.take().is_some() {
                    return Task::none();
                }
                if self.provider_editor.take().is_some() {
                    return Task::none();
                }
                if self.mcp_server_editor_open {
                    self.mcp_server_editor_open = false;
                    return Task::none();
                }
                if self.workbench_ai_capability_panel_open {
                    self.workbench_ai_capability_panel_open = false;
                    return Task::none();
                }
                if self.workbench_reasoning_picker_open {
                    self.workbench_reasoning_picker_open = false;
                    return Task::none();
                }
                if self.workbench_notice_modal.take().is_some() {
                    return Task::none();
                }
                if self.workbench_model_picker_open {
                    self.workbench_model_picker_open = false;
                    self.workbench_model_picker_filter.clear();
                    self.workbench_model_picker_highlighted_index = None;
                    self.workbench_model_picker_scroll_offset = 0.0;
                    self.workbench_model_picker_viewport_height =
                        WORKBENCH_MODEL_PICKER_LIST_HEIGHT;
                    return Task::none();
                }
                self.begin_close_thinking_modal();
                self.begin_close_tool_modal();
                Task::none()
            }
            Message::ConfirmTopLevelOverlay => {
                if self.workbench_image_preview_modal.is_some() {
                    return Task::none();
                }
                if self.ai_capability_vendor_editor.is_some() {
                    return self.save_ai_capability_vendor_editor();
                }
                if self.ai_capability_vendor_picker.is_some() {
                    return Task::none();
                }
                if self.ai_capability_target_picker.is_some() {
                    return self.apply_ai_capability_target_picker_selections();
                }
                if self.ai_capability_routing_picker.is_some() {
                    return Task::none();
                }
                if self.provider_model_override_editor.is_some() {
                    return Task::done(Message::SaveProviderModelOverrideEditor);
                }
                if self.provider_model_picker.is_some() {
                    return Task::done(Message::SaveProviderModelPicker);
                }
                if self.provider_editor.is_some() {
                    return Task::done(Message::SaveProviderConfiguration);
                }
                if self.mcp_server_editor_open {
                    return Task::done(Message::SaveMcpServerConfiguration);
                }
                if self.workbench_ai_capability_panel_open {
                    self.workbench_ai_capability_panel_open = false;
                    return Task::none();
                }
                if self.workbench_reasoning_picker_open {
                    self.workbench_reasoning_picker_open = false;
                    return Task::none();
                }
                if self.workbench_notice_modal.is_some() {
                    return self.confirm_workbench_notice_modal();
                }
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let Some(choice) = self.highlighted_workbench_model_choice() else {
                    return Task::none();
                };

                self.request_workbench_model_choice(choice)
            }
            Message::WindowFileHovered(window_id, path) => {
                if self.main_window_id != Some(window_id)
                    || !self.active_workbench_supports_any_attachments()
                {
                    return Task::none();
                }
                self.push_composer_drag_hover_label(&path);
                Task::none()
            }
            Message::WindowFilesHoverLeft(window_id) => {
                if self.main_window_id == Some(window_id) {
                    self.composer_drag_hover_labels.clear();
                }
                Task::none()
            }
            Message::WindowFileDropped(window_id, path) => {
                if self.main_window_id != Some(window_id) {
                    return Task::none();
                }
                self.composer_drag_hover_labels.clear();
                if !self.active_workbench_supports_any_attachments() {
                    let notice = String::from(
                        "当前模型未声明可用附件输入模态，请切换到支持音频、图片、视频或 PDF 的模型后再试。",
                    );
                    self.show_workbench_notice_modal("当前模型不支持附件", notice.clone());
                    self.status_message = notice;
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                if self.composer_attachments.len() >= MAX_COMPOSER_ATTACHMENTS {
                    self.show_composer_attachment_limit_notice("拖拽文件");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                let attachment_workspace_path = self.selected_attachment_workspace_path();
                match stage_composer_attachment_paths(
                    &attachment_workspace_path,
                    std::slice::from_ref(&path),
                ) {
                    Ok(attachments) => {
                        self.append_imported_composer_attachments(attachments, "拖拽")
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal(
                            Self::composer_attachment_error_title(&error),
                            error.clone(),
                        );
                        self.status_message = format!("处理拖拽附件失败：{error}");
                        operation::focus(COMPOSER_EDITOR_ID)
                    }
                }
            }
            Message::OpenComposerContextMenu => self.open_composer_context_menu(),
            Message::OpenTaskProjectContextMenu(project_id) => {
                self.open_task_project_context_menu(project_id)
            }
            Message::OpenTaskSessionContextMenu(session_id) => {
                self.open_task_session_context_menu(session_id)
            }
            Message::OpenArchivedTaskProjectContextMenu(project_id) => {
                self.open_archived_task_project_context_menu(project_id)
            }
            Message::OpenArchivedTaskSessionContextMenu(session_id) => {
                self.open_archived_task_session_context_menu(session_id)
            }
            Message::OpenTranscriptContextMenu(request) => {
                self.open_transcript_context_menu(request)
            }
            Message::CloseWorkbenchContextMenu => {
                self.dismiss_workbench_context_menu();
                Task::none()
            }
            Message::InvokeWorkbenchContextMenuItem(index) => {
                self.invoke_workbench_context_menu_item(index)
            }
            Message::ComposerContextClipboardLoaded(result) => {
                self.composer_context_clipboard_text = result
                    .ok()
                    .map(|text| text.as_ref().clone())
                    .filter(|text| composer_clipboard_text_is_pastable(Some(text)));
                self.refresh_open_composer_context_menu();
                Task::none()
            }
            Message::ComposerScrollbarPointerMoved(pointer_y) => {
                self.composer_scrollbar_pointer_y = Some(pointer_y);
                self.apply_composer_scrollbar_pointer_position(pointer_y);
                Task::none()
            }
            Message::StartComposerScrollbarDrag => {
                let Some(layout) = self.composer_scrollbar_layout() else {
                    return Task::none();
                };
                let pointer_y = self.composer_scrollbar_pointer_y.unwrap_or(8.0);
                let local_track_y = (pointer_y - 8.0).clamp(0.0, layout.track_height);
                let thumb_bottom = layout.thumb_top + layout.thumb_height;
                let grab_offset =
                    if local_track_y >= layout.thumb_top && local_track_y <= thumb_bottom {
                        local_track_y - layout.thumb_top
                    } else {
                        (layout.thumb_height / 2.0).clamp(0.0, layout.thumb_height)
                    };

                self.composer_scrollbar_drag_grab_offset = Some(grab_offset);
                self.apply_composer_scrollbar_pointer_position(pointer_y);
                Task::none()
            }
            Message::ComposerContextMenuSelectAll => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                let task = self.apply_composer_editor_action(text_editor::Action::SelectAll);
                Task::batch([task, operation::focus(COMPOSER_EDITOR_ID)])
            }
            Message::ComposerContextMenuCopy => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                if let Some(selection) = self
                    .composer_buffer
                    .selection()
                    .filter(|text| composer_selection_has_content(Some(text)))
                {
                    Task::batch([
                        clipboard::write(selection).discard(),
                        operation::focus(COMPOSER_EDITOR_ID),
                    ])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::ComposerContextMenuCut => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                if let Some(selection) = self
                    .composer_buffer
                    .selection()
                    .filter(|text| composer_selection_has_content(Some(text)))
                {
                    let action_task = self.apply_composer_editor_action(text_editor::Action::Edit(
                        text_editor::Edit::Backspace,
                    ));
                    Task::batch([
                        clipboard::write(selection).discard(),
                        action_task,
                        operation::focus(COMPOSER_EDITOR_ID),
                    ])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::ComposerContextMenuPaste => {
                self.workbench_context_menu = None;
                let clipboard_text = self.composer_context_clipboard_text.take();
                if let Some(text) = clipboard_text {
                    let action_task = self.apply_composer_editor_action(text_editor::Action::Edit(
                        text_editor::Edit::Paste(std::sync::Arc::new(text)),
                    ));
                    Task::batch([action_task, operation::focus(COMPOSER_EDITOR_ID)])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::TranscriptContextMenuCopySelection(selected_text) => {
                self.workbench_context_menu = None;
                self.status_message = String::from("已复制转录选中文本。");
                clipboard::write(selected_text).discard()
            }
            Message::TranscriptContextMenuSelectAll(surface_id) => {
                self.workbench_context_menu = None;
                self.selectable_transcript_command_revision = self
                    .selectable_transcript_command_revision
                    .saturating_add(1);
                self.selectable_transcript_command = Some(SelectableTranscriptCommand::SelectAll {
                    surface_id,
                    revision: self.selectable_transcript_command_revision,
                });
                Task::none()
            }
            Message::EditorActionPerformed(action) => {
                self.editor_content.perform(action);
                Task::none()
            }
            Message::SettingsSectionSelected(section) => {
                self.selected_settings_section = section;
                if section.belongs_to_ai_capabilities() {
                    self.settings_ai_capability_tree_expanded = true;
                }
                if section.belongs_to_model_settings() {
                    self.settings_model_tree_expanded = true;
                }
                self.status_message = format!("当前设置分区已切换到“{}”。", section.title());
                Task::none()
            }
            Message::ToggleModelSettingsTree => {
                self.settings_model_tree_expanded = !self.settings_model_tree_expanded;
                Task::none()
            }
            Message::ToggleAiCapabilitySettingsTree => {
                self.settings_ai_capability_tree_expanded =
                    !self.settings_ai_capability_tree_expanded;
                Task::none()
            }
            Message::ProviderCatalogSelected(provider) => {
                self.selected_provider = provider;
                if self.provider_editor.is_some() {
                    self.reload_selected_provider_form();
                    self.provider_notice = format!(
                        "已切换到供应商：{}。",
                        provider_config::provider_summary(&self.selected_provider.id)
                    );
                }
                Task::none()
            }
            Message::ProviderFieldChanged(field, value) => {
                self.provider_form.set(field, value);
                Task::none()
            }
            Message::OpenProviderEditorForCreate => {
                self.open_provider_editor_for_create();
                Task::none()
            }
            Message::OpenProviderEditorForEdit(provider_id) => {
                if let Err(error) = self.open_provider_editor_for_edit(&provider_id) {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderEditor => {
                self.provider_editor = None;
                Task::none()
            }
            Message::SaveProviderConfiguration => {
                let provider_id = self.selected_provider.id.clone();
                let provider_name = provider_config::provider_summary(&provider_id);
                let should_open_model_picker = self
                    .provider_editor
                    .as_ref()
                    .is_some_and(|editor| editor.is_new);
                if let Err(error) = self.save_selected_provider_configuration() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.refresh_runtime_foundation();
                self.provider_notice = format!("已保存 `{provider_name}` 供应商配置。");
                self.status_message = format!("供应商 `{provider_name}` 配置已写入本地文件。");
                self.provider_editor = None;
                if should_open_model_picker {
                    if let Err(error) = self.open_provider_model_picker(&provider_id) {
                        self.provider_notice = error.clone();
                        self.status_message = error;
                    } else if let Some(state) = self.provider_model_picker.as_mut() {
                        let default_enabled_ids =
                            provider_config::supported_provider_by_id(&provider_id)
                                .map(|definition| {
                                    provider_config::default_enabled_model_ids(&definition.models)
                                        .into_iter()
                                        .collect::<BTreeSet<_>>()
                                })
                                .unwrap_or_default();
                        state
                            .draft_enabled_model_ids_by_provider
                            .insert(provider_id.clone(), default_enabled_ids);
                    }
                }
                Task::none()
            }
            Message::OpenProviderModelPicker(provider_id) => {
                if let Err(error) = self.open_provider_model_picker(&provider_id) {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderModelPicker => {
                self.provider_model_picker = None;
                Task::none()
            }
            Message::ProviderModelPickerProviderSelected(provider_id) => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    state.selected_provider_id = provider_id;
                    state.filter.clear();
                }
                Task::none()
            }
            Message::ProviderModelPickerFilterChanged(value) => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    state.filter = value;
                }
                Task::none()
            }
            Message::ProviderModelPickerModelToggled {
                provider_id,
                model_id,
                enabled,
            } => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    if state.selected_provider_id == provider_id {
                        let draft_ids = state
                            .draft_enabled_model_ids_by_provider
                            .entry(provider_id)
                            .or_insert_with(BTreeSet::new);
                        if enabled {
                            draft_ids.insert(model_id);
                        } else {
                            draft_ids.remove(&model_id);
                        }
                    }
                }
                Task::none()
            }
            Message::ProviderModelPickerSelectAllVisible => {
                let visible_ids = self
                    .filtered_provider_model_picker_catalog()
                    .into_iter()
                    .map(|model| model.id)
                    .collect::<Vec<_>>();
                if let Some(state) = self.provider_model_picker.as_mut() {
                    let draft_ids = state
                        .draft_enabled_model_ids_by_provider
                        .entry(state.selected_provider_id.clone())
                        .or_insert_with(BTreeSet::new);
                    for model_id in visible_ids {
                        draft_ids.insert(model_id);
                    }
                }
                Task::none()
            }
            Message::ProviderModelPickerClearAllVisible => {
                let visible_ids = self
                    .filtered_provider_model_picker_catalog()
                    .into_iter()
                    .map(|model| model.id)
                    .collect::<Vec<_>>();
                if let Some(state) = self.provider_model_picker.as_mut() {
                    let draft_ids = state
                        .draft_enabled_model_ids_by_provider
                        .entry(state.selected_provider_id.clone())
                        .or_insert_with(BTreeSet::new);
                    for model_id in visible_ids {
                        draft_ids.remove(&model_id);
                    }
                }
                Task::none()
            }
            Message::SaveProviderModelPicker => {
                let provider_id = self
                    .provider_model_picker
                    .as_ref()
                    .map(|state| state.selected_provider_id.clone())
                    .unwrap_or_default();
                if let Err(error) = self.save_provider_model_picker() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.provider_model_picker = None;
                self.refresh_runtime_foundation();
                self.provider_notice = format!(
                    "已保存 `{}` 的启用模型配置。",
                    provider_config::provider_summary(&provider_id)
                );
                self.status_message = self.provider_notice.clone();
                Task::none()
            }
            Message::RemoveConfiguredProvider(provider_id) => {
                let provider_name = provider_config::provider_summary(&provider_id);
                self.show_workbench_confirmation_modal(
                    format!("确认移除模型供应商 `{provider_name}`"),
                    "移除后，该供应商的接入配置与已启用模型都会一并删除；如果它正作为当前模型来源，也会同步清空当前选择。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveConfiguredProvider { provider_id },
                );
                Task::none()
            }
            Message::OpenProviderModelOverrideEditor {
                provider_id,
                model_id,
            } => {
                if let Err(error) =
                    self.open_provider_model_override_editor(&provider_id, &model_id)
                {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderModelOverrideEditor => {
                self.provider_model_override_editor = None;
                Task::none()
            }
            Message::ProviderModelOverrideReasoningChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.reasoning = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideAttachmentChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.attachment = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideInputModalityToggled { modality, enabled } => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    match modality.as_str() {
                        "text" => editor.supports_text = enabled,
                        "image" => editor.supports_image = enabled,
                        "audio" => editor.supports_audio = enabled,
                        "video" => editor.supports_video = enabled,
                        "pdf" => editor.supports_pdf = enabled,
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::ProviderModelOverrideContextLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.context_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideInputLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.input_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideOutputLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.output_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideReasoningProfileAction(action) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.reasoning_profile_buffer.perform(action);
                }
                Task::none()
            }
            Message::SaveProviderModelOverrideEditor => {
                let provider_id = self
                    .provider_model_override_editor
                    .as_ref()
                    .map(|editor| editor.provider_id.clone())
                    .unwrap_or_default();
                let model_name = self
                    .provider_model_override_editor
                    .as_ref()
                    .map(|editor| editor.model_name.clone())
                    .unwrap_or_default();
                if let Err(error) = self.save_provider_model_override_editor() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.provider_model_override_editor = None;
                self.refresh_runtime_foundation();
                let reasoning_note = self.reconcile_workbench_reasoning_effort_for_active_model();
                let temperature_note = self.reconcile_workbench_temperature_for_active_model();
                let mut notes = Vec::new();
                if let Some(note) = reasoning_note {
                    notes.push(note);
                }
                if let Some(note) = temperature_note {
                    notes.push(note);
                }
                self.provider_notice = format!(
                    "已保存 `{}` 下模型 `{model_name}` 的覆盖配置。",
                    provider_config::provider_summary(&provider_id)
                );
                self.status_message = if notes.is_empty() {
                    self.provider_notice.clone()
                } else {
                    format!("{} {}", self.provider_notice, notes.join(" "))
                };
                Task::none()
            }
            Message::WebSearchRoutingModeSelected(mode) => {
                self.ai_capability_routing_picker = None;
                self.ai_tool_configuration.web_search.mode = mode;
                self.persist_ai_tool_configuration_change(format!(
                    "web-search 路由已切换为「{}」并自动保存。",
                    mode.label()
                ));
                Task::none()
            }
            Message::UnderstandImageRoutingModeSelected(mode) => {
                self.ai_capability_routing_picker = None;
                self.ai_tool_configuration.understand_image.mode = mode;
                self.persist_ai_tool_configuration_change(format!(
                    "图像理解路由已切换为「{}」并自动保存。",
                    mode.label()
                ));
                Task::none()
            }
            Message::OpenAiCapabilityRoutingPicker(capability_kind) => {
                self.ai_capability_routing_picker =
                    Some(AiCapabilityRoutingPickerState { capability_kind });
                Task::none()
            }
            Message::CloseAiCapabilityRoutingPicker => {
                self.ai_capability_routing_picker = None;
                Task::none()
            }
            Message::AiCapabilityTargetEnabledToggled {
                capability_kind,
                vendor_id,
                target_alias,
                enabled,
            } => {
                match self.set_ai_capability_target_enabled(
                    capability_kind,
                    &vendor_id,
                    &target_alias,
                    enabled,
                ) {
                    Ok(true) => {
                        let state_label = if enabled { "启用" } else { "停用" };
                        self.persist_ai_tool_configuration_change(format!(
                            "已{state_label}“{}”中的 `{}/{}` 并自动保存。",
                            capability_kind.label(),
                            vendor_id,
                            target_alias
                        ));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.ai_tool_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::RemoveAiCapabilityVendor(vendor_id) => {
                let vendor_name = self
                    .ai_tool_configuration
                    .vendor_by_id(&vendor_id)
                    .map(|vendor| vendor.name.clone())
                    .unwrap_or_else(|| {
                        provider_config::ai_capability_vendor_display_name(&vendor_id)
                    });
                self.show_workbench_confirmation_modal(
                    format!("确认移除共享能力供应商 `{vendor_name}`"),
                    "移除后，这个供应商下所有已配置接口都会一起删除，并立即写回当前能力扩展配置。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveAiCapabilityVendor { vendor_id },
                );
                Task::none()
            }
            Message::OpenAiCapabilityVendorPicker => {
                self.ai_capability_vendor_picker = Some(AiCapabilityVendorPickerState::default());
                Task::none()
            }
            Message::CloseAiCapabilityVendorPicker => {
                self.ai_capability_vendor_picker = None;
                Task::none()
            }
            Message::AiCapabilityVendorPickerFilterChanged(value) => {
                if let Some(state) = self.ai_capability_vendor_picker.as_mut() {
                    state.filter = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorPickerVendorSelected(vendor_id) => {
                self.ai_capability_vendor_picker = None;
                self.open_ai_capability_vendor_editor(&vendor_id, true);
                Task::none()
            }
            Message::OpenAiCapabilityVendorEditor(vendor_id) => {
                self.open_ai_capability_vendor_editor(&vendor_id, false);
                Task::none()
            }
            Message::CloseAiCapabilityVendorEditor => {
                self.ai_capability_vendor_editor = None;
                Task::none()
            }
            Message::AiCapabilityVendorEditorApiKeyChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.api_key = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorConnectionModeChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.connection_mode = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorBaseUrlChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.base_url = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorRegionChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.region = value;
                }
                Task::none()
            }
            Message::SaveAiCapabilityVendorEditor => self.save_ai_capability_vendor_editor(),
            Message::OpenAiCapabilityTargetPicker(capability_kind) => {
                self.open_ai_capability_target_picker(capability_kind)
            }
            Message::CloseAiCapabilityTargetPicker => {
                self.ai_capability_target_picker = None;
                Task::none()
            }
            Message::AiCapabilityTargetPickerVendorSelected(vendor_id) => {
                if let Some(picker) = self.ai_capability_target_picker.as_mut() {
                    picker.selected_vendor_id = Some(vendor_id);
                }
                Task::none()
            }
            Message::AiCapabilityTargetPickerTargetToggled {
                vendor_id,
                target_alias,
                enabled,
            } => {
                self.stage_ai_capability_target_picker_toggle(&vendor_id, &target_alias, enabled);
                Task::none()
            }
            Message::AiCapabilityTargetPickerTargetModelChanged {
                vendor_id,
                target_alias,
                model_id,
            } => {
                self.stage_ai_capability_target_picker_model(&vendor_id, &target_alias, model_id);
                Task::none()
            }
            Message::SaveAiCapabilityTargetPicker => {
                self.apply_ai_capability_target_picker_selections()
            }
            Message::StartAiCapabilityRouteDrag {
                capability_kind,
                target_selector,
            } => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_ai_capability_route_drag = Some(PendingAiCapabilityRouteDrag {
                        capability_kind,
                        target_selector,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::AiCapabilityRouteDropPreviewMoved {
                capability_kind,
                target_selector,
                position,
            } => {
                let Some(dragged_target_selector) =
                    self.dragging_ai_capability_route_target.as_ref()
                else {
                    return Task::none();
                };
                let Some(pending_drag) = self.pending_ai_capability_route_drag.as_ref() else {
                    return Task::none();
                };
                if pending_drag.capability_kind != capability_kind {
                    return Task::none();
                }
                if dragged_target_selector == &target_selector {
                    self.active_ai_capability_route_drop_target =
                        Some(AiCapabilityRouteDropTarget::Before(target_selector));
                    return Task::none();
                }
                let route_entries = self
                    .ai_tool_configuration
                    .resolved_configured_targets_for_kind(capability_kind);
                self.active_ai_capability_route_drop_target = Some(
                    if position.y <= AI_CAPABILITY_ROUTE_TARGET_ROW_HEIGHT / 2.0 {
                        AiCapabilityRouteDropTarget::Before(target_selector)
                    } else {
                        let target_index = route_entries
                            .iter()
                            .position(|entry| entry.target_selector == target_selector)
                            .unwrap_or(route_entries.len());
                        let next_target_selector = route_entries
                            .get(target_index.saturating_add(1))
                            .map(|entry| entry.target_selector.clone());
                        if let Some(next_target_selector) = next_target_selector {
                            if next_target_selector == *dragged_target_selector {
                                AiCapabilityRouteDropTarget::Before(target_selector)
                            } else {
                                AiCapabilityRouteDropTarget::Before(next_target_selector)
                            }
                        } else {
                            AiCapabilityRouteDropTarget::End
                        }
                    },
                );
                Task::none()
            }
            Message::AiCapabilityRouteTailHovered {
                capability_kind,
                is_hovered,
            } => {
                if !is_hovered {
                    if self.dragging_ai_capability_route_target.is_none() {
                        self.active_ai_capability_route_drop_target = None;
                    }
                    return Task::none();
                }
                if self.dragging_ai_capability_route_target.is_some()
                    && self
                        .pending_ai_capability_route_drag
                        .as_ref()
                        .is_some_and(|drag| drag.capability_kind == capability_kind)
                {
                    self.active_ai_capability_route_drop_target =
                        Some(AiCapabilityRouteDropTarget::End);
                }
                Task::none()
            }
            Message::RemoveAiCapabilityRouteTarget {
                capability_kind,
                target_selector,
            } => {
                if self.remove_ai_capability_route_target(capability_kind, &target_selector) {
                    self.persist_ai_tool_configuration_change(format!(
                        "已从“{}”中移除 `{}` 并自动保存。",
                        capability_kind.label(),
                        target_selector
                    ));
                }
                Task::none()
            }
            Message::OpenWorkbenchAiCapabilityPanel => {
                self.workbench_ai_capability_panel_open = true;
                Task::none()
            }
            Message::CloseWorkbenchAiCapabilityPanel => {
                self.workbench_ai_capability_panel_open = false;
                Task::none()
            }
            Message::WorkbenchAiCapabilitySectionSelected(section) => {
                self.workbench_ai_capability_section = section;
                Task::none()
            }
            Message::WorkbenchAiCapabilitySectionEnabledToggled { section, enabled } => {
                match section.capability_kind() {
                    Some(capability_kind) => {
                        let usable_target_count = self
                            .ai_tool_configuration
                            .usable_targets_for_kind(capability_kind)
                            .len();
                        if usable_target_count == 0 {
                            self.set_workbench_ai_capability_enabled(section, false);
                            self.status_message = format!(
                                "“{}”当前没有已启用的可用目标，请先在右侧内容区至少勾选一个模型后再开启。",
                                section.label()
                            );
                            return Task::none();
                        }
                        self.set_workbench_ai_capability_enabled(section, enabled);
                        let configured_target_count = self
                            .ai_tool_configuration
                            .resolved_configured_targets_for_kind(capability_kind)
                            .len();
                        if configured_target_count > 0 {
                            let state_label = if enabled { "启用" } else { "关闭" };
                            self.persist_ai_tool_configuration_change(format!(
                                "已{state_label}“{}”并自动保存。已保留既有目标勾选与顺序偏好。",
                                capability_kind.label()
                            ));
                            return Task::none();
                        } else {
                            self.status_message = format!(
                                "“{}”当前还没有可快速切换的已配置接口，请先在左侧能力页中完成配置。",
                                section.label()
                            );
                        }
                    }
                    None => {
                        self.status_message = format!(
                            "“{}”入口已预留，当前还没有可快速切换的真实能力目标。",
                            section.label()
                        );
                    }
                }
                Task::none()
            }
            Message::WorkbenchModelSelected(choice) => self.request_workbench_model_choice(choice),
            Message::OpenWorkbenchModelPicker => self.open_workbench_model_picker(),
            Message::CloseWorkbenchModelPicker => {
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.workbench_model_picker_scroll_offset = 0.0;
                self.workbench_model_picker_viewport_height = WORKBENCH_MODEL_PICKER_LIST_HEIGHT;
                Task::none()
            }
            Message::OpenWorkbenchReasoningPicker => {
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.workbench_reasoning_picker_open = true;
                Task::none()
            }
            Message::CloseWorkbenchReasoningPicker => {
                self.workbench_reasoning_picker_open = false;
                Task::none()
            }
            Message::WorkbenchModelPickerScrolled(viewport) => {
                self.workbench_model_picker_scroll_offset = viewport.absolute_offset().y.max(0.0);
                self.workbench_model_picker_viewport_height = viewport.bounds().height.max(0.0);
                Task::none()
            }
            Message::WorkbenchModelPickerFilterChanged(value) => {
                self.workbench_model_picker_filter = value;
                self.workbench_model_picker_highlighted_index =
                    self.preferred_workbench_model_picker_index();
                self.clamp_workbench_model_picker_highlight();
                self.workbench_model_picker_scroll_offset = 0.0;
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
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
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
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
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
            }
            Message::ActiveAgentSelected(choice) => {
                let selected_agent_id = if choice.id == SYSTEM_DEFAULT_AGENT_CHOICE_ID {
                    None
                } else if choice.id == PLAN_AGENT_ID || choice.id == EXPLORE_AGENT_ID {
                    self.execution_mode = ExecutionMode::Plan;
                    Some(choice.id.clone())
                } else if self.agent_profile_catalog.agents.contains_key(&choice.id) {
                    Some(choice.id.clone())
                } else {
                    self.status_message = format!(
                        "未找到可切换的代理 `{}`，无法切换当前激活代理。",
                        choice.label
                    );
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
                self.status_message = if choice.id == PLAN_AGENT_ID || choice.id == EXPLORE_AGENT_ID
                {
                    format!(
                        "当前激活代理已切换为 `{}`，执行模式已固定为 Plan。",
                        choice.label
                    )
                } else {
                    format!("当前激活代理已切换为 `{}`。", choice.label)
                };
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
                self.mcp_server_editor_open = true;
                self.mcp_oauth_callback_input.clear();
                self.mcp_oauth_authorization_url.clear();
                self.mcp_oauth_pending_state = None;
                self.mcp_oauth_flow_in_flight = false;
                self.mcp_notice.clear();
                self.load_selected_mcp_server_oauth_status_task()
            }
            Message::CloseMcpServerEditor => {
                self.mcp_server_editor_open = false;
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
            Message::McpServerArgRowChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.args_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerArgRow => {
                self.mcp_server_form
                    .args_rows
                    .push(McpTextRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerArgRow(index) => {
                if self.mcp_server_form.args_rows.len() > 1
                    && index < self.mcp_server_form.args_rows.len()
                {
                    self.mcp_server_form.args_rows.remove(index);
                }
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
            Message::McpServerHeaderKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerHeaderValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerHeaderRow => {
                self.mcp_server_form
                    .header_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerHeaderRow(index) => {
                if self.mcp_server_form.header_rows.len() > 1
                    && index < self.mcp_server_form.header_rows.len()
                {
                    self.mcp_server_form.header_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerHeaderFromEnvKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_from_env_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerHeaderFromEnvValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.header_from_env_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerHeaderFromEnvRow => {
                self.mcp_server_form
                    .header_from_env_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerHeaderFromEnvRow(index) => {
                if self.mcp_server_form.header_from_env_rows.len() > 1
                    && index < self.mcp_server_form.header_from_env_rows.len()
                {
                    self.mcp_server_form.header_from_env_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerOAuthClientIdChanged(value) => {
                self.mcp_server_form.oauth_client_id = value;
                Task::none()
            }
            Message::McpServerOAuthClientSecretChanged(value) => {
                self.mcp_server_form.oauth_client_secret = value;
                Task::none()
            }
            Message::McpServerOAuthScopesChanged(value) => {
                self.mcp_server_form.oauth_scopes_text = value;
                Task::none()
            }
            Message::McpServerOAuthRedirectUriChanged(value) => {
                self.mcp_server_form.oauth_redirect_uri = value;
                Task::none()
            }
            Message::McpServerCwdChanged(value) => {
                self.mcp_server_form.cwd = value;
                Task::none()
            }
            Message::McpServerEnvKeyChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_rows.get_mut(index) {
                    row.key = value;
                }
                Task::none()
            }
            Message::McpServerEnvValueChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerEnvRow => {
                self.mcp_server_form
                    .env_rows
                    .push(McpKeyValueRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerEnvRow(index) => {
                if self.mcp_server_form.env_rows.len() > 1
                    && index < self.mcp_server_form.env_rows.len()
                {
                    self.mcp_server_form.env_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerEnvPassthroughChanged(index, value) => {
                if let Some(row) = self.mcp_server_form.env_passthrough_rows.get_mut(index) {
                    row.value = value;
                }
                Task::none()
            }
            Message::AddMcpServerEnvPassthroughRow => {
                self.mcp_server_form
                    .env_passthrough_rows
                    .push(McpTextRowState::default());
                Task::none()
            }
            Message::RemoveMcpServerEnvPassthroughRow(index) => {
                if self.mcp_server_form.env_passthrough_rows.len() > 1
                    && index < self.mcp_server_form.env_passthrough_rows.len()
                {
                    self.mcp_server_form.env_passthrough_rows.remove(index);
                }
                Task::none()
            }
            Message::McpServerOAuthCallbackInputChanged(value) => {
                self.mcp_oauth_callback_input = value;
                Task::none()
            }
            Message::McpServerEnabledToggled(enabled) => {
                self.mcp_server_form.enabled = enabled;
                Task::none()
            }
            Message::CreateMcpServer => {
                self.selected_mcp_server_id = None;
                self.mcp_server_editor_open = true;
                self.mcp_server_form = McpServerFormState {
                    enabled: true,
                    ..McpServerFormState::default()
                };
                self.mcp_oauth_status = String::from("未配置 OAuth");
                self.mcp_oauth_callback_input.clear();
                self.mcp_oauth_authorization_url.clear();
                self.mcp_oauth_pending_state = None;
                self.mcp_oauth_flow_in_flight = false;
                self.mcp_notice.clear();
                Task::none()
            }
            Message::SaveMcpServerConfiguration => {
                match self.save_selected_mcp_server_configuration() {
                    Ok(server_id) => {
                        self.selected_mcp_server_id = Some(server_id);
                        self.mcp_server_editor_open = false;
                        self.reload_selected_mcp_server_form();
                        self.mcp_oauth_callback_input.clear();
                        self.mcp_oauth_authorization_url.clear();
                        self.mcp_oauth_pending_state = None;
                        self.mcp_oauth_flow_in_flight = false;
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
            Message::RequestRemoveMcpServerConfiguration(server_id) => {
                let server_name = self
                    .provider_configuration
                    .mcp
                    .servers
                    .get(&server_id)
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| String::from("未命名服务器"));
                self.show_workbench_confirmation_modal(
                    format!("确认移除 MCP `{server_name}`"),
                    "移除后，该 MCP 服务器的本地配置会被立即删除；如果它正处于编辑状态，对应编辑器也会一起关闭。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveMcpServerConfiguration { server_id },
                );
                Task::none()
            }
            Message::StartSelectedMcpServerOAuth => {
                let Some(server_id) = self.selected_mcp_server_id.clone() else {
                    self.mcp_notice = String::from("请先选择并保存一个支持远端连接的 MCP 服务器。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                };
                let Some(entry) = self.selected_mcp_server_entry() else {
                    self.mcp_notice = String::from("当前选中的 MCP 服务器不存在，请先保存配置。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                };
                if matches!(entry.transport, PersistedMcpTransportKind::Stdio) {
                    self.mcp_notice =
                        String::from("stdio 类型 MCP 服务器不需要宿主侧 OAuth 浏览器授权。");
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                }
                if entry.oauth.is_none() {
                    self.mcp_notice = String::from(
                        "当前 MCP 服务器尚未配置 OAuth 信息，请先填写 redirect URI 或客户端参数并保存。",
                    );
                    self.status_message = self.mcp_notice.clone();
                    return Task::none();
                }
                self.mcp_oauth_flow_in_flight = true;
                self.mcp_notice = format!("正在为 MCP 服务器 `{server_id}` 生成 OAuth 授权链接。");
                self.status_message = self.mcp_notice.clone();
                self.start_selected_mcp_server_oauth_task()
            }
            Message::SelectedMcpServerOAuthChallengeReady(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                match result {
                    Ok((authorization_url, oauth_state)) => {
                        self.mcp_oauth_authorization_url = authorization_url.clone();
                        self.mcp_oauth_pending_state = Some(oauth_state.clone());
                        self.mcp_oauth_flow_in_flight = true;
                        self.mcp_notice = String::from(
                            "OAuth 授权链接已生成，正在尝试打开默认浏览器；若自动回调失败，可粘贴浏览器最终跳转 URL 或查询串完成授权。",
                        );
                        self.status_message = self.mcp_notice.clone();

                        let browser_task = Task::perform(
                            open_external_url_in_browser(authorization_url),
                            Message::McpServerOAuthBrowserOpened,
                        );
                        let callback_task = self
                            .selected_mcp_server_oauth_redirect_uri()
                            .and_then(|redirect_uri| {
                                resolve_loopback_oauth_redirect_port(&redirect_uri)
                            })
                            .map(|port| {
                                Task::perform(
                                    capture_loopback_oauth_callback(
                                        port,
                                        oauth_state,
                                        Duration::from_secs(300),
                                    ),
                                    move |callback_result| {
                                        Message::SelectedMcpServerOAuthCallbackCaptured(
                                            server_id.clone(),
                                            callback_result,
                                        )
                                    },
                                )
                            });

                        if let Some(callback_task) = callback_task {
                            Task::batch([browser_task, callback_task])
                        } else {
                            browser_task
                        }
                    }
                    Err(error) => {
                        self.mcp_oauth_flow_in_flight = false;
                        self.mcp_notice = format!("生成 MCP OAuth 授权链接失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                        Task::none()
                    }
                }
            }
            Message::McpServerOAuthBrowserOpened(result) => {
                if let Err(error) = result {
                    self.mcp_notice = format!(
                        "已生成 OAuth 授权链接，但自动打开浏览器失败：{error}。请手动复制并打开下方链接。"
                    );
                    self.status_message = self.mcp_notice.clone();
                }
                Task::none()
            }
            Message::SelectedMcpServerOAuthCallbackCaptured(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                if self.mcp_oauth_pending_state.is_none() {
                    return Task::none();
                }
                match result {
                    Ok(callback_input) => {
                        self.mcp_oauth_callback_input = callback_input.clone();
                        let Ok(callback_payload) =
                            parse_hosted_oauth_callback_input(&callback_input)
                        else {
                            self.mcp_oauth_flow_in_flight = false;
                            self.mcp_notice = String::from(
                                "本地回调已捕获，但解析失败，请手动检查并重新发起授权。",
                            );
                            self.status_message = self.mcp_notice.clone();
                            return Task::none();
                        };
                        self.mcp_notice = String::from("已捕获本地 OAuth 回调，正在完成授权。");
                        self.status_message = self.mcp_notice.clone();
                        self.finish_selected_mcp_server_oauth_task(
                            callback_payload.authorization_code,
                            callback_payload.oauth_state,
                        )
                    }
                    Err(error) => {
                        self.mcp_notice = format!(
                            "等待本地 OAuth 回调失败：{error}。请改为粘贴浏览器最终跳转 URL 或查询串完成授权。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        Task::none()
                    }
                }
            }
            Message::CompleteSelectedMcpServerOAuthFromInput => {
                let parsed = parse_hosted_oauth_callback_input(&self.mcp_oauth_callback_input);
                match parsed {
                    Ok(callback_payload) => {
                        if let Some(expected_state) = self.mcp_oauth_pending_state.as_deref() {
                            if callback_payload.oauth_state != expected_state {
                                self.mcp_notice = String::from(
                                    "手动回调中的 `state` 与当前等待值不一致，请重新发起授权。",
                                );
                                self.status_message = self.mcp_notice.clone();
                                self.mcp_oauth_flow_in_flight = false;
                                return Task::none();
                            }
                        }
                        self.mcp_notice = String::from("正在完成 MCP OAuth 授权。");
                        self.status_message = self.mcp_notice.clone();
                        self.finish_selected_mcp_server_oauth_task(
                            callback_payload.authorization_code,
                            callback_payload.oauth_state,
                        )
                    }
                    Err(error) => {
                        self.mcp_oauth_flow_in_flight = false;
                        self.mcp_notice = error.clone();
                        self.status_message = error;
                        Task::none()
                    }
                }
            }
            Message::SelectedMcpServerOAuthFinished(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                self.mcp_oauth_flow_in_flight = false;
                match result {
                    Ok(status_label) => {
                        self.mcp_oauth_status = status_label.clone();
                        self.mcp_notice = format!(
                            "MCP 服务器 `{server_id}` OAuth 授权已完成，当前状态：{status_label}。正在重启运行时并刷新连接。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        self.mcp_oauth_pending_state = None;
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = format!("完成 MCP OAuth 授权失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                    }
                }
                Task::none()
            }
            Message::RemoveSelectedMcpServerOAuthAuthorization => {
                self.mcp_notice = String::from("正在清理当前 MCP 服务器的 OAuth 凭据。");
                self.status_message = self.mcp_notice.clone();
                self.remove_selected_mcp_server_oauth_task()
            }
            Message::SelectedMcpServerOAuthAuthorizationRemoved(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                match result {
                    Ok(()) => {
                        self.mcp_oauth_status = String::from("未认证");
                        self.mcp_notice = format!(
                            "MCP 服务器 `{server_id}` 的 OAuth 凭据已清理，正在重启运行时。"
                        );
                        self.status_message = self.mcp_notice.clone();
                        self.mcp_oauth_pending_state = None;
                        self.mcp_oauth_authorization_url.clear();
                        self.mcp_oauth_callback_input.clear();
                        self.mcp_oauth_flow_in_flight = false;
                        return self.restart_mcp_runtime_task();
                    }
                    Err(error) => {
                        self.mcp_notice = format!("清理 MCP OAuth 凭据失败：{error}");
                        self.status_message = self.mcp_notice.clone();
                    }
                }
                Task::none()
            }
            Message::SelectedMcpServerOAuthStatusLoaded(server_id, result) => {
                if self.selected_mcp_server_id.as_deref() != Some(server_id.as_str()) {
                    return Task::none();
                }
                self.mcp_oauth_status =
                    result.unwrap_or_else(|error| format!("状态读取失败：{error}"));
                Task::none()
            }
            Message::OpenTaskProjectSettings(project_id) => {
                let Some(project) = self.task_project_by_id(&project_id) else {
                    self.status_message = String::from("无法打开项目设置：项目已经不存在。");
                    return Task::none();
                };
                let project_title = project.title.clone();
                self.project_settings_project_id = Some(project_id);
                self.project_attachment_cleanup_date_picker_open = false;
                self.project_attachment_cleanup_preview = None;
                self.project_attachment_cleanup_last_report = None;
                self.project_attachment_cleanup_notice =
                    format!("正在管理项目 `{project_title}` 的历史附件缓存。");
                Task::none()
            }
            Message::CloseTaskProjectSettings => {
                self.project_settings_project_id = None;
                self.project_attachment_cleanup_date_picker_open = false;
                Task::none()
            }
            Message::OpenProjectAttachmentCleanupDatePicker => {
                self.project_attachment_cleanup_date_picker_open = true;
                Task::none()
            }
            Message::CancelProjectAttachmentCleanupDatePicker => {
                self.project_attachment_cleanup_date_picker_open = false;
                Task::none()
            }
            Message::ProjectAttachmentCleanupDateSelected(date) => {
                self.project_attachment_cleanup_cutoff_date = date;
                self.project_attachment_cleanup_date_picker_open = false;
                self.project_attachment_cleanup_preview = None;
                self.project_attachment_cleanup_notice =
                    String::from("项目附件清理截止日期已更新，可先预览再执行实际清理。");
                Task::none()
            }
            Message::ProjectAttachmentCleanupKindToggled(kind, enabled) => {
                match kind {
                    ConversationAttachmentKind::Image => {
                        self.project_attachment_cleanup_selection.image = enabled;
                    }
                    ConversationAttachmentKind::Audio => {
                        self.project_attachment_cleanup_selection.audio = enabled;
                    }
                    ConversationAttachmentKind::Video => {
                        self.project_attachment_cleanup_selection.video = enabled;
                    }
                    ConversationAttachmentKind::Pdf => {
                        self.project_attachment_cleanup_selection.pdf = enabled;
                    }
                }
                self.project_attachment_cleanup_preview = None;
                self.project_attachment_cleanup_notice = format!(
                    "已更新{}清理开关，可先预览当前项目再执行。",
                    kind.display_label()
                );
                Task::none()
            }
            Message::PreviewProjectAttachmentCleanup => {
                let Some(project) = self.project_settings_task_project().cloned() else {
                    self.project_attachment_cleanup_notice =
                        String::from("无法预览清理范围：项目已经不存在。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                let cutoff_date = self.project_attachment_cleanup_cutoff_date.to_string();
                match preview_attachment_cleanup(
                    &project.path,
                    &cutoff_date,
                    self.project_attachment_cleanup_selection,
                ) {
                    Ok(preview) => {
                        let preview_summary =
                            self.format_attachment_cleanup_preview_summary(&preview);
                        self.project_attachment_cleanup_preview = Some(preview);
                        self.project_attachment_cleanup_last_report = None;
                        self.project_attachment_cleanup_notice = format!(
                            "已生成项目 `{}` 的附件清理预览：{preview_summary}",
                            project.title
                        );
                        self.status_message = self.project_attachment_cleanup_notice.clone();
                    }
                    Err(error) => {
                        self.project_attachment_cleanup_preview = None;
                        self.project_attachment_cleanup_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ExecuteProjectAttachmentCleanup => {
                let Some(project) = self.project_settings_task_project().cloned() else {
                    self.project_attachment_cleanup_notice =
                        String::from("无法执行清理：项目已经不存在。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                let cutoff_date = self.project_attachment_cleanup_cutoff_date.to_string();
                let Some(preview) = self.project_attachment_cleanup_preview.clone() else {
                    self.project_attachment_cleanup_notice =
                        String::from("请先预览当前项目的附件清理范围，再执行实际清理。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                if preview.cutoff_date_text != cutoff_date {
                    self.project_attachment_cleanup_preview = None;
                    self.project_attachment_cleanup_notice =
                        String::from("清理截止日期已经变化，请重新预览后再执行。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                if preview.matched_asset_count == 0 {
                    self.project_attachment_cleanup_notice =
                        String::from("当前预览没有匹配到可清理附件，无需执行清理。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                let current_preview = match preview_attachment_cleanup(
                    &project.path,
                    &cutoff_date,
                    self.project_attachment_cleanup_selection,
                ) {
                    Ok(current_preview) => current_preview,
                    Err(error) => {
                        self.project_attachment_cleanup_notice = error.clone();
                        self.status_message = error;
                        return Task::none();
                    }
                };
                if current_preview != preview {
                    let preview_summary =
                        self.format_attachment_cleanup_preview_summary(&current_preview);
                    self.project_attachment_cleanup_preview = Some(current_preview);
                    self.project_attachment_cleanup_notice = format!(
                        "项目 `{}` 的附件清理范围已经变化，已刷新预览：{preview_summary}。请确认后再次执行。",
                        project.title
                    );
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                let preview_summary = self.format_attachment_cleanup_preview_summary(&preview);
                let confirmation = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("确认清理项目附件缓存")
                    .set_description(format!(
                        "将清理项目 `{}` 的历史附件缓存：{}\n\n清理后，旧会话中的这些附件会撤销回放，不再发送给模型。是否继续？",
                        project.title, preview_summary
                    ))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();
                if confirmation != rfd::MessageDialogResult::Yes {
                    self.project_attachment_cleanup_notice =
                        String::from("已取消项目附件缓存清理。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                match execute_attachment_cleanup(
                    &project.path,
                    &cutoff_date,
                    self.project_attachment_cleanup_selection,
                ) {
                    Ok(report) => {
                        let report_summary = self.format_attachment_cleanup_report_summary(&report);
                        self.project_attachment_cleanup_last_report = Some(report);
                        self.project_attachment_cleanup_preview = preview_attachment_cleanup(
                            &project.path,
                            &cutoff_date,
                            self.project_attachment_cleanup_selection,
                        )
                        .ok();
                        self.project_attachment_cleanup_notice = format!(
                            "项目 `{}` 的历史附件缓存清理已完成：{report_summary}",
                            project.title
                        );
                        self.status_message = self.project_attachment_cleanup_notice.clone();
                    }
                    Err(error) => {
                        self.project_attachment_cleanup_notice = error.clone();
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
                if self.selected_agent_requires_plan_mode() {
                    self.execution_mode = ExecutionMode::Plan;
                    self.status_message =
                        String::from("当前选中的是分析代理，执行模式已固定为 Plan。");
                } else {
                    self.execution_mode = mode;
                    self.status_message = format!("执行模式已切换为 {mode}。");
                }
                Task::none()
            }
            Message::WorkbenchReasoningEffortSelected(option) => {
                self.workbench_reasoning_picker_open = false;
                self.workbench_reasoning_effort_preference = option.effort;
                self.workbench_reasoning_effort = option.effort;
                match self.persist_workbench_reasoning_effort_preference() {
                    Ok(()) => {
                        self.status_message = format!("思考模式已切换为 {}。", option.label);
                    }
                    Err(error) => {
                        self.provider_notice = error.clone();
                        self.status_message = format!(
                            "思考模式已切换为 {}，但保存到配置文件失败：{}",
                            option.label, error
                        );
                    }
                }
                Task::none()
            }
            Message::CycleExecutionMode => {
                if self.selected_agent_requires_plan_mode() {
                    self.execution_mode = ExecutionMode::Plan;
                    self.status_message =
                        String::from("当前选中的是分析代理，执行模式已固定为 Plan。");
                } else {
                    self.execution_mode = match self.execution_mode {
                        ExecutionMode::Build => ExecutionMode::Plan,
                        ExecutionMode::Plan => ExecutionMode::Build,
                    };
                    self.status_message = format!("执行模式已切换为 {}。", self.execution_mode);
                }
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
                if self
                    .workspace_history_search
                    .pending_navigation
                    .as_ref()
                    .is_some_and(|pending| pending.session_id != session_id)
                {
                    self.workspace_history_search.pending_navigation = None;
                }
                let has_pending_search_navigation = self
                    .workspace_history_search
                    .pending_navigation
                    .as_ref()
                    .is_some_and(|pending| pending.session_id == session_id);
                self.selected_session = session_id;
                self.hovered_task_session = Some(session_id);
                self.clear_pending_prompt_drag_state();
                self.sync_workspace_path_with_selected_session();
                self.clear_transcript_drag_auto_scrolls();
                self.active_subagent_panel = None;
                self.rebuild_subagent_task_cache_for_session(session_id);
                self.session_agent_view_follow_tail
                    .insert(session_id, !has_pending_search_navigation);
                self.session_streaming_thinking_follow_tail
                    .insert(session_id, true);
                // Capture the selected session title to keep footer feedback specific.
                // 记录被选中的会话标题，保证底部反馈足够具体。
                let selected_title = self
                    .selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"));

                self.status_message = format!("当前会话已切换为：{selected_title}。");
                let projection_task =
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id);
                Task::batch([
                    self.schedule_session_hydration(session_id),
                    projection_task,
                    if has_pending_search_navigation {
                        Task::none()
                    } else {
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    },
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                    self.maybe_continue_workspace_history_search_navigation(session_id),
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
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
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
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
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
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::OpenSelectedSessionThinkingModal(block_id) => {
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };
                self.open_thinking_modal(session_id, block_id);
                Task::none()
            }
            Message::OpenSelectedSessionToolModal(block_id) => {
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };
                let selected_root_session_id = self.selected_session().map(|session| session.id);
                if selected_root_session_id == Some(session_id)
                    && let Some(task_id) =
                        self.resolve_task_session_id_from_rendered_tool_block(session_id, &block_id)
                    && self.should_open_subagent_panel_from_tool_card(session_id, task_id)
                    && self.open_subagent_task_panel(session_id, task_id)
                {
                    return Task::none();
                }
                self.open_tool_modal(session_id, block_id);
                Task::none()
            }
            Message::OpenSelectedSessionSubagentPanel(task_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                if !self.open_subagent_task_panel(session_id, task_id) {
                    self.status_message =
                        String::from("当前子代理详情已不可恢复，请从 task 工具卡片查看返回信息。");
                }
                Task::none()
            }
            Message::SubagentNavigationHoverChanged(is_expanded) => {
                self.subagent_navigation_expanded = is_expanded;
                Task::none()
            }
            Message::StopSelectedSubagentTask => {
                let Some(state) = self.selected_subagent_task_panel_state().cloned() else {
                    self.status_message = String::from("当前没有可停止的子代理面板。");
                    return Task::none();
                };
                if !self.subagent_task_is_actively_running(state.task_id) {
                    self.status_message = String::from("当前子代理已经结束，无需再次停止。");
                    return Task::none();
                }
                let stop_summary = subagent_task_stopped_summary_text();
                match self
                    .runtime_foundation
                    .task_runtime()
                    .stop_task(state.task_id)
                {
                    Ok(true) => {
                        self.mark_subagent_task_stopped(
                            state.parent_session_id,
                            state.task_id,
                            stop_summary,
                        );
                        self.refresh_subagent_navigation_round_visibility(state.parent_session_id);
                        self.status_message = String::from("已停止当前子代理调用。");
                        Task::batch([
                            self.persist_workspace_snapshot_task(),
                            self.maybe_follow_selected_session_transcript_tail(state.task_id),
                        ])
                    }
                    Ok(false) => {
                        self.status_message =
                            String::from("当前子代理已不在执行中，可直接查看已有结果。");
                        Task::none()
                    }
                    Err(error) => {
                        self.status_message = format!("停止子代理失败：{error}");
                        Task::none()
                    }
                }
            }
            Message::ReturnToSelectedSessionTranscript => {
                let parent_session_id = self.selected_session().map(|session| session.id);
                self.active_subagent_panel = None;
                if let Some(parent_session_id) = parent_session_id {
                    self.refresh_subagent_navigation_round_visibility(parent_session_id);
                }
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
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };

                self.session_agent_view_follow_tail.insert(session_id, true);
                operation::snap_to_end(transcript_scrollable_id(session_id))
            }
            Message::LoadOlderSessionHistoryRequested(session_id) => {
                if self.selected_session().map(|session| session.id) != Some(session_id) {
                    return Task::none();
                }
                if self.loading_older_session_history_ids.contains(&session_id) {
                    self.status_message = String::from("更早会话历史正在加载中。");
                    return Task::none();
                }
                if !self.session_history_has_more.contains(&session_id) {
                    self.status_message = String::from("当前会话没有更多可加载历史。");
                    return Task::none();
                }

                self.status_message = String::from("正在加载更早一段会话历史。");
                self.schedule_older_session_history_load(session_id)
            }
            Message::OpenWorkspaceHistorySearchPanel => self.open_workspace_history_search_panel(),
            Message::CloseWorkspaceHistorySearchPanel => {
                self.workspace_history_search.panel_open = false;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                Task::none()
            }
            Message::WorkspaceHistorySearchQueryChanged(query) => {
                self.workspace_history_search.query = query;
                self.workspace_history_search.error = None;
                if self.workspace_history_search.query.trim().is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.pending_navigation = None;
                }
                Task::none()
            }
            Message::SubmitWorkspaceHistorySearch => {
                if self.workspace_history_search.loading {
                    self.status_message = String::from("工作台历史搜索正在进行中。");
                    return Task::none();
                }
                let query = self.workspace_history_search.query.trim();
                if query.is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.error = None;
                    self.status_message = String::from("请输入要搜索的历史关键词。");
                    return self.open_workspace_history_search_panel();
                }

                self.workspace_history_search.panel_open = true;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                self.status_message = String::from("正在搜索工作台历史。");
                self.schedule_workspace_history_search(false)
            }
            Message::LoadMoreWorkspaceHistorySearchResults => {
                if self.workspace_history_search.loading {
                    self.status_message = String::from("工作台历史搜索正在进行中。");
                    return Task::none();
                }
                let Some(current_page) = self.workspace_history_search.results.as_ref() else {
                    self.status_message = String::from("当前还没有可继续加载的搜索结果。");
                    return Task::none();
                };
                if !current_page.has_more {
                    self.status_message = String::from("当前搜索结果已经全部加载完成。");
                    return Task::none();
                }
                if self.workspace_history_search.query.trim() != current_page.query {
                    self.status_message =
                        String::from("搜索词已变更，请先重新搜索，再继续加载更多结果。");
                    return Task::none();
                }

                self.workspace_history_search.error = None;
                self.status_message = String::from("正在继续加载更多工作台历史结果。");
                self.schedule_workspace_history_search(true)
            }
            Message::WorkspaceHistorySearchLoaded { append, result } => {
                self.workspace_history_search.loading = false;
                self.workspace_history_search.loading_more = false;
                match result {
                    Ok(page) => {
                        let WorkspaceHistorySearchPage {
                            query,
                            hits,
                            limit_reached,
                            total_hits,
                            next_offset,
                            has_more,
                        } = page;
                        let hit_count = hits.len();
                        self.workspace_history_search.error = None;
                        if append {
                            if let Some(existing_page) =
                                self.workspace_history_search.results.as_mut()
                            {
                                if existing_page.query == query {
                                    existing_page.hits.extend(hits);
                                    existing_page.limit_reached = limit_reached;
                                    existing_page.total_hits = total_hits;
                                    existing_page.next_offset = next_offset;
                                    existing_page.has_more = has_more;
                                } else {
                                    self.workspace_history_search.results =
                                        Some(WorkspaceHistorySearchPage {
                                            query,
                                            hits,
                                            limit_reached,
                                            total_hits,
                                            next_offset,
                                            has_more,
                                        });
                                }
                            } else {
                                self.workspace_history_search.results =
                                    Some(WorkspaceHistorySearchPage {
                                        query,
                                        hits,
                                        limit_reached,
                                        total_hits,
                                        next_offset,
                                        has_more,
                                    });
                            }
                            let loaded_count = self
                                .workspace_history_search
                                .results
                                .as_ref()
                                .map(|merged_page| merged_page.hits.len())
                                .unwrap_or(0);
                            let total_hits = self
                                .workspace_history_search
                                .results
                                .as_ref()
                                .map(|merged_page| merged_page.total_hits)
                                .unwrap_or(loaded_count);
                            self.status_message = format!(
                                "已继续加载 {hit_count} 条结果，当前共显示 {loaded_count}/{total_hits} 条 BM25 命中。"
                            );
                        } else {
                            self.workspace_history_search.results =
                                Some(WorkspaceHistorySearchPage {
                                    query,
                                    hits,
                                    limit_reached,
                                    total_hits,
                                    next_offset,
                                    has_more,
                                });
                            self.status_message =
                                format!("工作台历史搜索完成，找到 {total_hits} 条 BM25 命中。");
                        }
                    }
                    Err(error) => {
                        self.workspace_history_search.error = Some(error.clone());
                        self.status_message = format!("工作台历史搜索失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台历史搜索失败", error);
                    }
                }

                Task::none()
            }
            Message::ToggleWorkspaceHistorySearchProjectPicker => {
                self.workspace_history_search.panel_open = true;
                let next_open = !self.workspace_history_search.project_picker_open;
                self.workspace_history_search.project_picker_open = next_open;
                if next_open {
                    self.workspace_history_search.project_filter.clear();
                    operation::focus(WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID)
                } else {
                    self.workspace_history_search.project_filter.clear();
                    operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
                }
            }
            Message::WorkspaceHistorySearchProjectFilterChanged(value) => {
                self.workspace_history_search.project_filter = value;
                Task::none()
            }
            Message::WorkspaceHistorySearchScopeSelected(scope) => {
                self.workspace_history_search.scope = scope;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
            }
            Message::ConfirmWorkspaceHistorySearchScopeSelection => {
                let Some(option) = self
                    .filtered_workspace_history_search_scope_options()
                    .into_iter()
                    .next()
                else {
                    self.status_message = String::from("没有匹配的项目范围可供选择。");
                    return operation::focus(WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID);
                };

                self.workspace_history_search.scope = option.scope;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
            }
            Message::OpenWorkspaceHistorySearchHit {
                session_id,
                message_id,
                ordinal,
                turn_index,
            } => {
                self.workspace_history_search.active_highlight = None;
                self.workspace_history_search.pending_navigation =
                    Some(PendingWorkspaceHistorySearchNavigation {
                        session_id,
                        message_id,
                        ordinal,
                        turn_index,
                    });
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.panel_open = false;
                self.status_message = String::from("正在打开搜索命中并定位对应历史位置。");

                if self.selected_session().map(|session| session.id) == Some(session_id) {
                    self.maybe_continue_workspace_history_search_navigation(session_id)
                } else {
                    Task::done(Message::SessionSelected(session_id))
                }
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
            Message::SelectedSessionTranscriptFollowTailTick => {
                self.perform_pending_transcript_tail_follow_tick()
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
            Message::SelectedSessionThinkingOverlayFollowTailTick => {
                self.perform_pending_thinking_overlay_tail_follow_tick()
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
                self.rebuild_diff_code_buffers();
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
            Message::StartRun => self.submit_selected_session_prompt(false),
            Message::PendingPromptQueueInsertNow(entry_id) => {
                self.dispatch_pending_prompt_entry_now(&entry_id)
            }
            Message::PendingPromptQueueEdit(entry_id) => self.edit_pending_prompt_entry(&entry_id),
            Message::PendingPromptQueueRemove(entry_id) => {
                self.remove_pending_prompt_entry(&entry_id)
            }
            Message::StartPendingPromptQueueDrag(entry_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_pending_prompt_drag = Some(PendingPendingPromptDrag {
                        session_id,
                        entry_id,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::PendingPromptQueueDropPreviewMoved(target_entry_id, position) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                let Some(dragged_entry_id) = self.dragging_pending_prompt_entry_id.as_ref() else {
                    return Task::none();
                };
                if let Some(pending_drag) = self.pending_pending_prompt_drag.as_ref() {
                    if pending_drag.session_id != session_id {
                        return Task::none();
                    }
                }
                if dragged_entry_id == &target_entry_id {
                    self.active_pending_prompt_drop_target =
                        Some(PendingPromptDropTarget::Before(target_entry_id));
                    return Task::none();
                }
                self.active_pending_prompt_drop_target =
                    Some(if position.y <= COMPOSER_PENDING_QUEUE_ROW_HEIGHT / 2.0 {
                        PendingPromptDropTarget::Before(target_entry_id)
                    } else {
                        let queue_entries = self.session_pending_prompt_panel_queue(session_id);
                        let target_index = queue_entries
                            .iter()
                            .position(|entry| entry.id == target_entry_id)
                            .unwrap_or(queue_entries.len());
                        let next_entry_id = queue_entries
                            .get(target_index.saturating_add(1))
                            .map(|entry| entry.id.clone());
                        if let Some(next_entry_id) = next_entry_id {
                            if next_entry_id == *dragged_entry_id {
                                PendingPromptDropTarget::Before(target_entry_id)
                            } else {
                                PendingPromptDropTarget::Before(next_entry_id)
                            }
                        } else {
                            PendingPromptDropTarget::End
                        }
                    });
                Task::none()
            }
            Message::PendingPromptQueueTailHovered(is_hovered) => {
                if !is_hovered {
                    if self.dragging_pending_prompt_entry_id.is_none() {
                        self.active_pending_prompt_drop_target = None;
                    }
                    return Task::none();
                }
                if self.dragging_pending_prompt_entry_id.is_some() {
                    self.active_pending_prompt_drop_target = Some(PendingPromptDropTarget::End);
                }
                Task::none()
            }
            Message::RunExecutionStreamEvent { run_id, event } => {
                let Some(session_id) = self.active_run_session_id_for_run_id(run_id) else {
                    return Task::none();
                };

                match event {
                    ExecutionStreamEvent::Prepared(preparation) => {
                        let detail = preparation.summary_rows().join(" | ");
                        self.replace_session_last_api_request_preview(
                            preparation.session_id,
                            preparation.provider_request_preview_markdown.clone(),
                        );
                        self.last_execution_preparation = Some(preparation.clone());
                        self.sync_context_usage_from_preparation(Some(&preparation));
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
                        self.request_selected_session_transcript_tail_follow(session_id);
                        self.request_selected_session_thinking_overlay_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                        self.dock_streaming_thinking_message(session_id);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        let detached_listener_task =
                            self.maybe_start_detached_subagent_task_listener(session_id, &event);
                        self.push_session_runtime_event(session_id, event);
                        Task::batch([
                            detached_listener_task,
                            self.maybe_follow_selected_session_transcript_tail(session_id),
                            self.maybe_start_next_pending_prompt(session_id, false),
                        ])
                    }
                    ExecutionStreamEvent::Finished(report) => {
                        self.clear_active_run_tracking(session_id);
                        self.apply_execution_run_report(report)
                    }
                }
            }
            Message::DetachedSubagentTaskEvent { task_id, event } => match event {
                DetachedTaskRuntimeEvent::ChildExecution(event) => match event {
                    ExecutionStreamEvent::Prepared(_) => Task::none(),
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.append_streaming_thinking_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        self.request_selected_session_thinking_overlay_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                        self.dock_streaming_thinking_message(session_id);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.push_session_runtime_event(session_id, event);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::Finished(report) => {
                        self.apply_execution_run_report(report)
                    }
                },
                DetachedTaskRuntimeEvent::ParentRuntime {
                    parent_session_id,
                    event,
                } => {
                    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
                    self.push_session_runtime_event(parent_session_id, event);
                    if is_finished {
                        self.detached_subagent_task_listeners.remove(&task_id);
                        self.refresh_subagent_navigation_round_visibility(parent_session_id);
                    }
                    Task::batch([
                        self.persist_workspace_snapshot_task(),
                        self.maybe_follow_selected_session_transcript_tail(parent_session_id),
                        self.maybe_start_next_pending_prompt(parent_session_id, false),
                    ])
                }
            },
            Message::StopRun => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    self.status_message = String::from("当前没有正在执行的 AI 调用。");
                    return Task::none();
                };
                self.stop_all_session_work(session_id)
            }
            Message::PaneFocused(pane) => {
                self.focus = Some(pane);
                Task::none()
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
                self.refresh_visible_diff_buffers();
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

    /// Returns the default search scope derived from the currently selected session, or global when no visible project is active.
    /// 返回根据当前选中会话推导出的默认搜索范围；若当前没有可见项目会话则回退到全局。
    fn default_workspace_history_search_scope(&self) -> WorkspaceHistorySearchScope {
        self.selected_session()
            .and_then(|session| session.project_id.clone())
            .filter(|project_id| self.task_project_by_id(project_id).is_some())
            .map(WorkspaceHistorySearchScope::Project)
            .unwrap_or(WorkspaceHistorySearchScope::Global)
    }

    /// Returns the human-readable label shown by the current workspace-history scope selector.
    /// 返回当前工作台历史范围选择器显示的人类可读标签。
    fn workspace_history_search_scope_label(&self) -> String {
        match &self.workspace_history_search.scope {
            WorkspaceHistorySearchScope::Global => String::from("全部项目"),
            WorkspaceHistorySearchScope::Project(project_id) => self
                .task_project_by_id(project_id)
                .map(|project| project.title.clone())
                .unwrap_or_else(|| String::from("全部项目")),
        }
    }

    /// Builds the full visible scope-option list used by the left-panel workspace-history search picker.
    /// 构建左侧工作台历史搜索选择器使用的完整可见范围选项列表。
    fn workspace_history_search_scope_options(&self) -> Vec<WorkspaceHistorySearchScopeOption> {
        let mut options = vec![WorkspaceHistorySearchScopeOption {
            scope: WorkspaceHistorySearchScope::Global,
            label: String::from("全部项目"),
            filter_text: String::from("全部项目 global all"),
        }];

        options.extend(self.task_projects.iter().map(|project| {
            WorkspaceHistorySearchScopeOption {
                scope: WorkspaceHistorySearchScope::Project(project.id.clone()),
                label: project.title.clone(),
                filter_text: format!("{} {}", project.title, project.path).to_lowercase(),
            }
        }));

        options
    }

    /// Returns the scope options filtered by the current project keyword text inside the left search panel.
    /// 返回根据左侧搜索面板当前项目关键字文本过滤后的范围选项集合。
    fn filtered_workspace_history_search_scope_options(
        &self,
    ) -> Vec<WorkspaceHistorySearchScopeOption> {
        let filter = self
            .workspace_history_search
            .project_filter
            .trim()
            .to_lowercase();
        let options = self.workspace_history_search_scope_options();
        if filter.is_empty() {
            return options;
        }

        options
            .into_iter()
            .filter(|option| option.filter_text.contains(&filter))
            .collect()
    }

    /// Opens the dedicated left-panel workspace-history search surface and focuses the main query input.
    /// 打开左侧专用工作台历史搜索表面，并聚焦主查询输入框。
    fn open_workspace_history_search_panel(&mut self) -> Task<Message> {
        let should_reset_scope = !self.workspace_history_search.panel_open
            || matches!(
                &self.workspace_history_search.scope,
                WorkspaceHistorySearchScope::Project(project_id)
                    if self.task_project_by_id(project_id).is_none()
            );
        if should_reset_scope {
            self.workspace_history_search.scope = self.default_workspace_history_search_scope();
        }
        self.workspace_history_search.panel_open = true;
        self.workspace_history_search.project_picker_open = false;
        self.workspace_history_search.project_filter.clear();

        operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
    }

    /// Continues one pending search-result navigation by hydrating older history until the target message becomes visible.
    /// 继续推进一条待完成的搜索结果定位流程，按需补载更早历史直到目标消息可见。
    fn maybe_continue_workspace_history_search_navigation(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        let Some(pending_navigation) = self
            .workspace_history_search
            .pending_navigation
            .clone()
            .filter(|pending| pending.session_id == session_id)
        else {
            return Task::none();
        };

        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return Task::none();
        }

        self.session_agent_view_follow_tail
            .insert(session_id, false);

        if self
            .session_message_entry(session_id, &pending_navigation.message_id)
            .is_some()
        {
            self.workspace_history_search.active_highlight =
                Some(WorkspaceHistorySearchHighlightTarget {
                    session_id,
                    message_id: pending_navigation.message_id.clone(),
                });
            self.workspace_history_search.pending_navigation = None;
            self.refresh_session_inline_agent_view_document_cache(session_id);
            self.status_message = format!(
                "已定位并高亮搜索命中：轮次 {} · 会话 {}。",
                pending_navigation.turn_index,
                self.selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"))
            );
            return operation::scroll_to(
                transcript_scrollable_id(session_id),
                scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: self.estimated_workspace_history_search_scroll_offset(
                        session_id,
                        &pending_navigation.message_id,
                    ),
                },
            );
        }

        if self.loading_session_ids.contains(&session_id)
            || self.loading_older_session_history_ids.contains(&session_id)
        {
            return Task::none();
        }

        if !self.hydrated_session_ids.contains(&session_id) {
            self.status_message = String::from("正在载入目标会话正文以定位搜索命中。");
            return self.schedule_session_hydration(session_id);
        }

        if self
            .session_history_earliest_ordinals
            .get(&session_id)
            .copied()
            .is_some_and(|earliest_ordinal| pending_navigation.ordinal >= earliest_ordinal)
        {
            self.workspace_history_search.active_highlight = None;
            self.workspace_history_search.pending_navigation = None;
            self.refresh_session_inline_agent_view_document_cache(session_id);
            self.status_message = String::from("已打开目标会话，但当前窗口中未恢复出该搜索命中。");
            return Task::none();
        }

        if self.session_history_has_more.contains(&session_id) {
            self.status_message = String::from("正在补载更早会话历史以定位搜索命中。");
            return self.schedule_older_session_history_load(session_id);
        }

        self.workspace_history_search.active_highlight = None;
        self.workspace_history_search.pending_navigation = None;
        self.refresh_session_inline_agent_view_document_cache(session_id);
        self.status_message = String::from("已加载完可见历史，但仍未定位到该搜索命中。");
        Task::none()
    }

    /// Estimates the transcript scroll offset needed to bring one searched message close to the viewport center.
    /// 估算将某条搜索命中消息带到视口附近所需的转录滚动偏移量。
    fn estimated_workspace_history_search_scroll_offset(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> f32 {
        let message_index = self
            .visible_session_messages_for_agent_view(session_id)
            .iter()
            .position(|message| message.id == message_id)
            .or_else(|| {
                self.session_messages.get(&session_id).and_then(|messages| {
                    messages.iter().position(|message| message.id == message_id)
                })
            })
            .unwrap_or(0);

        (message_index as f32 * WORKSPACE_HISTORY_SEARCH_SCROLL_ESTIMATED_ROW_HEIGHT - 48.0)
            .max(0.0)
    }

    /// Returns the current execution phase tracked for the provided session, defaulting to idle when no explicit state exists yet.
    /// 返回给定会话当前跟踪到的执行阶段；若尚无显式状态则默认视为空闲。
    fn session_run_phase_for(&self, session_id: SessionId) -> SessionRunPhase {
        self.session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle)
    }

    /// Replaces the main composer buffer with the provided prompt text and focuses the editor again.
    /// 使用给定提示词文本替换主提示词缓冲区，并重新聚焦编辑器。
    fn restore_prompt_into_composer(
        &mut self,
        prompt: String,
        attachments: Vec<ConversationAttachment>,
    ) -> Result<Task<Message>, String> {
        let workspace_path = self.selected_attachment_workspace_path();
        let composer_scope = self.composer_attachment_ref_scope();
        let previous_attachments = self.composer_attachments.clone();
        self.acquire_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            &attachments,
            "输入框",
        )?;
        self.push_composer_undo_snapshot();
        let released_previous_attachments = Self::staged_attachments_released_after_replacement(
            &previous_attachments,
            &attachments,
        );
        self.release_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            &released_previous_attachments,
            "输入框",
        );
        self.composer = prompt.clone();
        self.composer_buffer = text_editor::Content::with_text(&prompt);
        self.composer_attachments = attachments;
        self.composer_drag_hover_labels.clear();
        self.composer_scroll_offset = 0.0;
        self.composer_scrollbar_pointer_y = None;
        self.composer_scrollbar_drag_grab_offset = None;
        self.dismiss_workbench_context_menu();
        Ok(operation::focus(COMPOSER_EDITOR_ID))
    }

    /// Clears the floating composer input surface after one successful submit, queue, or interrupt action.
    /// 在一次成功的发送、排队或插队动作后清空浮动提示词输入表面。
    fn clear_composer_input(&mut self) {
        let workspace_path = self.selected_attachment_workspace_path();
        let composer_scope = self.composer_attachment_ref_scope();
        let previous_attachments = self.composer_attachments.clone();
        self.release_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            &previous_attachments,
            "输入框",
        );
        self.release_all_composer_undo_attachment_refs();
        self.composer.clear();
        self.composer_buffer = text_editor::Content::with_text("");
        self.composer_attachments.clear();
        self.composer_drag_hover_labels.clear();
        self.composer_scroll_offset = 0.0;
        self.composer_scrollbar_pointer_y = None;
        self.composer_scrollbar_drag_grab_offset = None;
        self.dismiss_workbench_context_menu();
    }

    /// Opens the reusable blocking workbench notice modal with a default confirmation action.
    /// 使用默认确认动作打开可复用阻塞工作台提示弹窗。
    fn show_workbench_notice_modal(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.workbench_notice_modal = Some(WorkbenchNoticeModalState {
            title: title.into(),
            body: body.into(),
            confirm_label: String::from("确认"),
            dismiss_label: None,
            confirm_action: WorkbenchNoticeConfirmAction::Acknowledge,
        });
    }

    /// Opens the reusable blocking workbench notice modal with an explicit confirm/cancel action pair.
    /// 使用明确的确认与取消动作打开可复用阻塞工作台提示弹窗。
    fn show_workbench_confirmation_modal(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        confirm_label: impl Into<String>,
        dismiss_label: impl Into<String>,
        confirm_action: WorkbenchNoticeConfirmAction,
    ) {
        self.workbench_notice_modal = Some(WorkbenchNoticeModalState {
            title: title.into(),
            body: body.into(),
            confirm_label: confirm_label.into(),
            dismiss_label: Some(dismiss_label.into()),
            confirm_action,
        });
    }

    /// Executes the domain action attached to the currently visible workbench notice modal.
    /// 执行当前可见工作台提示弹窗附带的业务确认动作。
    fn confirm_workbench_notice_modal(&mut self) -> Task<Message> {
        let Some(modal) = self.workbench_notice_modal.take() else {
            return Task::none();
        };

        match modal.confirm_action {
            WorkbenchNoticeConfirmAction::Acknowledge => Task::none(),
            WorkbenchNoticeConfirmAction::ApplyModelSwitchRemovingUnsupportedAttachments {
                choice,
            } => self.apply_confirmed_workbench_model_choice_after_attachment_cleanup(choice),
            WorkbenchNoticeConfirmAction::RemoveConfiguredProvider { provider_id } => {
                self.remove_configured_provider_after_confirmation(&provider_id)
            }
            WorkbenchNoticeConfirmAction::RemoveAiCapabilityVendor { vendor_id } => {
                self.remove_ai_capability_vendor_after_confirmation(&vendor_id)
            }
            WorkbenchNoticeConfirmAction::RemoveMcpServerConfiguration { server_id } => {
                self.remove_mcp_server_configuration_after_confirmation(&server_id)
            }
        }
    }

    /// Removes one configured model provider after the user explicitly confirms the destructive action.
    /// 在用户显式确认破坏性操作后移除一条模型供应商配置。
    fn remove_configured_provider_after_confirmation(
        &mut self,
        provider_id: &str,
    ) -> Task<Message> {
        let provider_name = provider_config::provider_summary(provider_id);
        self.provider_configuration.provider.remove(provider_id);
        if self
            .provider_configuration
            .model
            .as_deref()
            .and_then(provider_config::parse_active_model_reference)
            .is_some_and(|(active_provider_id, _)| active_provider_id == provider_id)
        {
            self.provider_configuration.model = None;
        }
        if let Err(error) = provider_config::save_configuration_file(&self.provider_configuration) {
            self.provider_notice = error.clone();
            self.status_message = error;
            return Task::none();
        }
        self.refresh_runtime_foundation();
        self.provider_notice = format!("已移除 `{provider_name}` 供应商配置。");
        self.status_message = format!("供应商 `{provider_name}` 配置已删除。");
        Task::none()
    }

    /// Removes one shared capability vendor after the user explicitly confirms the destructive action.
    /// 在用户显式确认破坏性操作后移除一条共享能力供应商配置。
    fn remove_ai_capability_vendor_after_confirmation(&mut self, vendor_id: &str) -> Task<Message> {
        let vendor_name = self
            .ai_tool_configuration
            .vendor_by_id(vendor_id)
            .map(|vendor| vendor.name.clone())
            .unwrap_or_else(|| provider_config::ai_capability_vendor_display_name(vendor_id));
        if self.remove_ai_capability_vendor(vendor_id) {
            self.persist_ai_tool_configuration_change(format!(
                "已移除共享能力供应商 `{vendor_name}` 并自动保存。"
            ));
        } else {
            self.ai_tool_notice = String::from("未找到要移除的共享能力供应商。");
            self.status_message = self.ai_tool_notice.clone();
        }
        Task::none()
    }

    /// Removes one configured MCP server after the user explicitly confirms the destructive action.
    /// 在用户显式确认破坏性操作后移除一条 MCP 服务器配置。
    fn remove_mcp_server_configuration_after_confirmation(
        &mut self,
        server_id: &str,
    ) -> Task<Message> {
        match self.remove_mcp_server_configuration_by_id(server_id) {
            Ok(removed_name) => {
                self.mcp_server_editor_open = false;
                self.reload_selected_mcp_server_form();
                self.mcp_oauth_status = String::from("未配置 OAuth");
                self.mcp_oauth_callback_input.clear();
                self.mcp_oauth_authorization_url.clear();
                self.mcp_oauth_pending_state = None;
                self.mcp_oauth_flow_in_flight = false;
                self.mcp_notice = format!("已移除 MCP `{removed_name}`。");
                self.status_message = format!("MCP `{removed_name}` 配置已删除。");
                self.restart_mcp_runtime_task()
            }
            Err(error) => {
                self.mcp_notice = error.clone();
                self.status_message = error;
                Task::none()
            }
        }
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
            self.hydrated_session_ids.remove(&session_id);
            self.dirty_session_payload_ids.remove(&session_id);
            self.dirty_session_message_ids.remove(&session_id);
            self.loading_session_ids.remove(&session_id);
            self.session_history_earliest_ordinals.remove(&session_id);
            self.session_history_has_more.remove(&session_id);
            self.loading_older_session_history_ids.remove(&session_id);
            self.remove_session_from_background_legacy_maintenance(session_id);
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
            self.hydrated_session_ids.remove(&session_id);
            self.dirty_session_payload_ids.remove(&session_id);
            self.dirty_session_message_ids.remove(&session_id);
            self.loading_session_ids.remove(&session_id);
            self.session_history_earliest_ordinals.remove(&session_id);
            self.session_history_has_more.remove(&session_id);
            self.loading_older_session_history_ids.remove(&session_id);
            self.remove_session_from_background_legacy_maintenance(session_id);
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
        // Deduplicate active and archived session summaries by stable session id and let active sessions win whenever overlap still exists in memory.
        // 按稳定会话标识去重活动与归档会话摘要，并在内存里仍有重叠时优先保留活动会话。
        let mut visible_session_seen = BTreeSet::new();
        let visible_sessions = self
            .sessions
            .iter()
            .filter(|session| visible_session_ids.contains(&session.id))
            .filter(|session| visible_session_seen.insert(session.id))
            .cloned()
            .collect::<Vec<_>>();
        let visible_session_ids = visible_sessions
            .iter()
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();
        let mut archived_session_seen = BTreeSet::new();
        let archived_sessions = self
            .archived_sessions
            .iter()
            .filter(|session| !visible_session_ids.contains(&session.id))
            .filter(|session| archived_session_seen.insert(session.id))
            .cloned()
            .collect::<Vec<_>>();
        let archived_session_ids = archived_sessions
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
                .or_else(|| archived_sessions.first().map(|session| session.id))
                .unwrap_or_default()
        };
        let replaceable_session_payload_ids = self
            .dirty_session_payload_ids
            .iter()
            .copied()
            .filter(|session_id| {
                visible_session_ids.contains(session_id)
                    || archived_session_ids.contains(session_id)
            })
            .collect::<BTreeSet<_>>();
        let replaceable_session_message_ids = self
            .dirty_session_message_ids
            .iter()
            .copied()
            .filter(|session_id| {
                visible_session_ids.contains(session_id)
                    || archived_session_ids.contains(session_id)
            })
            .collect::<BTreeSet<_>>();

        WorkspaceSnapshot {
            task_projects: self.task_projects.clone(),
            archived_task_projects: self.archived_task_projects.clone(),
            sessions: visible_sessions,
            archived_sessions: archived_sessions.clone(),
            session_messages: self
                .session_runtime_state
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_message_ids.contains(session_id))
                .map(|(session_id, state)| {
                    (
                        *session_id,
                        conversation_state_to_session_messages(&state.conversation_state),
                    )
                })
                .filter(|(_, messages)| !messages.is_empty())
                .collect(),
            archived_session_messages: self
                .archived_session_runtime_state
                .iter()
                .filter(|(session_id, _)| archived_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_message_ids.contains(session_id))
                .map(|(session_id, state)| {
                    (
                        *session_id,
                        conversation_state_to_session_messages(&state.conversation_state),
                    )
                })
                .filter(|(_, messages)| !messages.is_empty())
                .collect(),
            session_compactions: self
                .session_compactions
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_payload_ids.contains(session_id))
                .map(|(session_id, entries)| (*session_id, entries.clone()))
                .collect(),
            archived_session_compactions: self
                .archived_session_compactions
                .iter()
                .filter(|(session_id, _)| archived_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_payload_ids.contains(session_id))
                .map(|(session_id, entries)| (*session_id, entries.clone()))
                .collect(),
            session_runtime_state: self
                .session_runtime_state
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_payload_ids.contains(session_id))
                .map(|(session_id, state)| (*session_id, state.clone()))
                .collect(),
            archived_session_runtime_state: self
                .archived_session_runtime_state
                .iter()
                .filter(|(session_id, _)| archived_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_payload_ids.contains(session_id))
                .map(|(session_id, state)| (*session_id, state.clone()))
                .collect(),
            session_run_phases: self
                .session_run_phases
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .map(|(session_id, phase)| (*session_id, *phase))
                .collect(),
            expanded_task_projects: self.expanded_task_projects.clone(),
            selected_session,
            hydrated_session_ids: replaceable_session_payload_ids,
        }
    }

    /// Rebuilds the cached diff hunk payloads and refreshes visible buffers.
    /// 重建缓存 Diff hunk 载荷，并刷新可见缓冲区。
    fn rebuild_diff_code_buffers(&mut self) {
        self.diff_wrapped_hunks = rebuild_wrapped_diff_hunks(&self.diff_panel);
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

    /// Returns whether the currently selected runtime agent must always execute in Plan mode.
    /// 返回当前选中的运行时代理是否必须始终以 Plan 模式执行。
    fn selected_agent_requires_plan_mode(&self) -> bool {
        matches!(
            self.agent_profile_catalog.state.selected_agent_id.as_deref(),
            Some(agent_id) if agent_id == PLAN_AGENT_ID || agent_id == EXPLORE_AGENT_ID
        )
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
        if agent_profile::is_reserved_agent_identifier(&normalized_id) {
            return Err(String::from(
                "该 Agent 标识已被系统内置 Agent 保留，请显式填写一个不同的英文或 slug 标识。",
            ));
        }

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
                code_view::wrap_diff_hunk(hunk, 0).visual_lines.len() as f32
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
        self.push_visible_session_record_message_with_metadata(
            session_id,
            kind,
            record_kind,
            title,
            body,
            None,
            collapsed_by_default,
        )
    }

    /// Appends one visible transcript message together with optional structured metadata used only by transcript projections.
    /// 追加一条带可选结构化元数据的可见转录消息，且该元数据仅供转录投影使用。
    fn push_visible_session_record_message_with_metadata(
        &mut self,
        session_id: SessionId,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
        metadata: Option<Value>,
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
                metadata,
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
        let projection_entry = compact_tool_detail_message_for_projection(&materialized_entry);
        self.dirty_session_message_ids.insert(session_id);
        self.append_session_record_to_conversation(session_id, &materialized_entry);
        self.remember_tool_detail_message(session_id, &materialized_entry);
        self.session_message_raw_buffers.insert(
            message_id.clone(),
            text_editor::Content::with_text(&projection_entry.body),
        );
        let (markdown_content, markdown_buffer) =
            parse_selectable_markdown_body(&projection_entry.body, &self.app_theme);
        self.session_message_markdown
            .insert(message_id.clone(), markdown_content);
        self.session_message_markdown_buffers
            .insert(message_id.clone(), markdown_buffer);
        self.sync_session_messages_from_conversation_state(session_id);
        self.refresh_session_agent_view_buffers(session_id);
        message_id
    }

    /// Returns the immutable runtime-state snapshot for one visible or archived session when it exists.
    /// 返回某个可见或归档会话在存在时对应的只读运行时状态快照。
    fn session_runtime_state(&self, session_id: SessionId) -> Option<&SessionRuntimeStateEntry> {
        self.session_runtime_state
            .get(&session_id)
            .or_else(|| self.archived_session_runtime_state.get(&session_id))
    }

    /// Returns the immutable transcript message entry for one visible or archived session/message pair when it exists.
    /// 返回某个可见或归档会话/消息对在存在时对应的只读转录消息条目。
    fn session_message_entry(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<&SessionMessageEntry> {
        self.session_messages
            .get(&session_id)
            .or_else(|| self.archived_session_messages.get(&session_id))?
            .iter()
            .find(|message| message.id == message_id)
    }

    /// Stores every full tool-detail row from a message batch in the out-of-band detail cache.
    /// 将一批消息中的完整工具明细行保存到带外详情缓存。
    fn remember_tool_detail_messages(
        &mut self,
        session_id: SessionId,
        messages: &[SessionMessageEntry],
    ) {
        for message in messages {
            self.remember_tool_detail_message(session_id, message);
        }
    }

    /// Stores one full tool-detail row outside the main UI projection when it carries hidden payload text.
    /// 当单条工具明细行携带隐藏载荷文本时，将其保存到主界面投影之外。
    fn remember_tool_detail_message(
        &mut self,
        session_id: SessionId,
        message: &SessionMessageEntry,
    ) {
        if !session_message_is_tool_detail(message) {
            return;
        }
        self.session_tool_detail_messages
            .entry(session_id)
            .or_default()
            .insert(message.id.clone(), message.clone());
    }

    /// Returns the full tool-detail row for one session/message pair when it has been loaded or produced locally.
    /// 当指定会话/消息对已经加载或本地产生时，返回对应完整工具明细行。
    fn full_tool_detail_message_entry(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<&SessionMessageEntry> {
        self.session_tool_detail_messages
            .get(&session_id)?
            .get(message_id)
    }

    /// Builds the full modal-facing tool row, merging a matching result back into its call row when the visible card is merged.
    /// 构建面向弹层的完整工具行；当可见卡片已合并时，将匹配的返回结果重新合并到调用行中。
    fn full_tool_modal_message_entry(
        &self,
        session_id: SessionId,
        message: &SessionMessageEntry,
    ) -> SessionMessageEntry {
        let mut full_message = self
            .full_tool_detail_message_entry(session_id, &message.id)
            .cloned()
            .unwrap_or_else(|| message.clone());
        if full_message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            return full_message;
        }

        let Some(tool_call_id) = tool_display_message_call_id(&full_message) else {
            return full_message;
        };
        let Some(result_message) = self
            .session_tool_detail_messages
            .get(&session_id)
            .and_then(|messages| {
                messages.values().find(|candidate| {
                    candidate.record_kind == SessionMessageRecordKind::ToolResultOutput
                        && tool_display_message_call_id(candidate).as_deref()
                            == Some(tool_call_id.as_str())
                })
            })
            .cloned()
        else {
            return full_message;
        };

        merge_tool_result_into_display_entry(&mut full_message, result_message);
        full_message
    }

    /// Returns the session identifier that currently owns the visible transcript surface, whether it is the root session or one opened subagent panel.
    /// 返回当前可见转录表面所属的会话标识，无论该表面属于根会话还是已打开的子代理面板。
    fn active_transcript_surface_session_id(&self) -> Option<SessionId> {
        self.selected_subagent_task_panel_state()
            .map(|state| state.task_id)
            .or_else(|| self.selected_session().map(|session| session.id))
    }

    /// Appends one canonical record to the session standard state so transcript projection never becomes the source of truth.
    /// 向会话标准状态追加一条规范记录，确保转录投影永远不是事实源。
    fn append_session_record_to_conversation(
        &mut self,
        session_id: SessionId,
        entry: &SessionMessageEntry,
    ) {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: entry.id.clone(),
                kind: entry.kind,
                record_kind: entry.record_kind,
                visible: entry.visible,
                title: entry.title.clone(),
                body: entry.body.clone(),
                metadata: entry.metadata.clone(),
                created_at: entry.created_at.clone(),
            },
        );
        runtime_state.updated_at = entry.created_at.clone();
    }

    /// Rebuilds one session transcript projection from canonical records and stores it in the visible or archived cache bucket.
    /// 根据规范记录重建单个会话的转录投影，并将其存入可见或归档缓存桶。
    fn sync_session_messages_from_conversation_state(&mut self, session_id: SessionId) {
        let full_projected_messages = self
            .session_runtime_state(session_id)
            .map(|state| conversation_state_to_session_messages(&state.conversation_state))
            .unwrap_or_default();
        self.remember_tool_detail_messages(session_id, &full_projected_messages);
        let projected_messages = full_projected_messages
            .iter()
            .map(compact_tool_detail_message_for_projection)
            .collect::<Vec<_>>();
        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let existing_messages = if session_is_archived {
            self.archived_session_messages
                .get(&session_id)
                .cloned()
                .unwrap_or_default()
        } else {
            self.session_messages
                .get(&session_id)
                .cloned()
                .unwrap_or_default()
        };
        let merged_messages = merge_projected_session_messages_into_existing_window(
            &existing_messages,
            projected_messages,
        );

        if session_is_archived {
            self.session_messages.remove(&session_id);
            if merged_messages.is_empty() {
                self.archived_session_messages.remove(&session_id);
            } else {
                self.archived_session_messages
                    .insert(session_id, merged_messages);
            }
        } else {
            self.archived_session_messages.remove(&session_id);
            if merged_messages.is_empty() {
                self.session_messages.remove(&session_id);
            } else {
                self.session_messages.insert(session_id, merged_messages);
            }
        }
        self.rebuild_subagent_task_cache_for_session(session_id);
    }

    /// Appends one streamed body delta directly into the already projected visible transcript row so live rendering does not need to rebuild the whole session message list first.
    /// 直接将一段流式正文增量追加到已投影的可见转录行中，避免实时渲染先重建整份会话消息列表。
    fn append_stream_delta_to_projected_session_message(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) -> bool {
        if let Some(messages) = self.session_messages.get_mut(&session_id)
            && let Some(message) = messages.iter_mut().find(|message| message.id == message_id)
        {
            message.body.push_str(delta);
            return true;
        }

        if let Some(messages) = self.archived_session_messages.get_mut(&session_id)
            && let Some(message) = messages.iter_mut().find(|message| message.id == message_id)
        {
            message.body.push_str(delta);
            return true;
        }

        false
    }

    /// Appends one streamed body delta into an existing visible transcript message without rebuilding legacy per-message Markdown caches.
    /// 将一段流式正文增量追加到现有可见转录消息，但不重建旧版逐消息 Markdown 缓存。
    fn append_stream_delta_to_session_message(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        if !append_record_body_delta(&mut runtime_state.conversation_state, message_id, delta) {
            return;
        }
        runtime_state.updated_at = current_unix_millis_text();
        self.dirty_session_message_ids.insert(session_id);
        if !self.append_stream_delta_to_projected_session_message(session_id, message_id, delta) {
            self.sync_session_messages_from_conversation_state(session_id);
        }
        if !self.refresh_session_agent_view_message_blocks(session_id, message_id) {
            self.refresh_session_agent_view_buffers(session_id);
        }
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
        let record_kind = record.record_kind;
        let body = record.body.clone();
        let message_id = self.push_hidden_session_message(
            session_id,
            record.kind,
            record.record_kind,
            record.title,
            record.body,
        );
        self.maybe_mirror_hidden_record_to_session_conversation(
            session_id,
            record_kind,
            &message_id,
            &body,
        );
        message_id
    }

    /// Ensures the target session has one canonical conversation snapshot owned by the runtime-state layer.
    /// 确保目标会话拥有一份由运行时状态层持有的规范会话快照。
    fn ensure_session_conversation_state_loaded(&mut self, session_id: SessionId) {
        let _ = self.ensure_session_runtime_state_mut(session_id);
    }

    /// Returns the full canonical conversation state that should be forwarded into the next runtime call.
    /// 返回应转发给下一次运行时调用的完整规范会话状态。
    fn session_conversation_state(
        &self,
        session_id: SessionId,
    ) -> crate::conversation_protocol::SessionConversationState {
        self.session_runtime_state
            .get(&session_id)
            .map(|state| state.conversation_state.clone())
            .unwrap_or_default()
    }

    /// Appends the latest user prompt to the canonical conversation state before the real runtime starts.
    /// 在真实运行时启动前，将最新用户提示追加到规范会话状态。
    fn append_user_prompt_to_session_conversation(
        &mut self,
        session_id: SessionId,
        prompt: String,
        attachments: Vec<ConversationAttachment>,
        promoted_staging_scopes: Vec<String>,
    ) -> Result<(), String> {
        self.ensure_session_conversation_state_loaded(session_id);
        let parts = build_composer_input_parts(&prompt, &attachments);
        let (canonical_text, canonical_attachments) = composer_parts_to_user_payload(&parts);
        let protocol_message_id = format!(
            "session-{session_id}-protocol-user-{}",
            current_unix_millis_text()
        );
        let updated_at = current_unix_millis_text();
        let referenced_attachments = canonical_attachments.clone();
        let attachment_workspace_path = self.attachment_workspace_path_for_session(session_id);
        crate::attachment_asset_index::promote_staged_attachment_to_message_ref(
            &attachment_workspace_path,
            &session_id.to_string(),
            &protocol_message_id,
            &referenced_attachments,
            &promoted_staging_scopes,
        )
        .map_err(|error| format!("更新附件资产引用索引失败：{error}"))?;
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            append_user_text_message(
                &mut runtime_state.conversation_state,
                protocol_message_id.clone(),
                canonical_text,
                canonical_attachments,
            );
            runtime_state.updated_at = updated_at;
        }
        Ok(())
    }

    /// Mirrors selected hidden transcript records into the canonical conversation state so existing derived context remains effective.
    /// 将选定的隐藏转录记录镜像到规范会话状态，从而保留既有派生上下文效果。
    fn maybe_mirror_hidden_record_to_session_conversation(
        &mut self,
        session_id: SessionId,
        record_kind: SessionMessageRecordKind,
        message_id: &str,
        body: &str,
    ) {
        let should_mirror = matches!(record_kind, SessionMessageRecordKind::ExploreOutput);
        if !should_mirror {
            return;
        }

        self.ensure_session_conversation_state_loaded(session_id);
        let updated_at = current_unix_millis_text();
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            message_id.to_string(),
            body.to_string(),
        );
        runtime_state.updated_at = updated_at;
    }

    /// Rewrites the canonical conversation window after one transcript compaction summary is created.
    /// 在生成一条转录压缩摘要后改写规范会话窗口。
    fn compact_session_conversation_history(
        &mut self,
        session_id: SessionId,
        summary_message_id: &str,
        summary_body: &str,
        created_at: &str,
    ) {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        compact_conversation_with_summary(
            &mut runtime_state.conversation_state,
            summary_message_id.to_string(),
            summary_body.to_string(),
            created_at.to_string(),
            ContextCompactionPolicy::default(),
        );
        compact_conversation_records_after_summary(
            &mut runtime_state.conversation_state,
            summary_message_id,
            ContextCompactionPolicy::default(),
        );
        runtime_state.updated_at = created_at.to_string();
    }

    /// Returns the mutable runtime-state snapshot for the target session, creating one default row on demand.
    /// 返回目标会话对应的可变运行时状态快照，并在缺失时按需创建默认行。
    fn ensure_session_runtime_state_mut(
        &mut self,
        session_id: SessionId,
    ) -> &mut SessionRuntimeStateEntry {
        self.hydrated_session_ids.insert(session_id);
        self.dirty_session_payload_ids.insert(session_id);
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

    /// Synchronizes the visible workbench context bar from the latest execution preparation snapshot.
    /// 根据最近一次执行准备快照同步可见工作台上下文条。
    fn sync_context_usage_from_preparation(&mut self, preparation: Option<&ExecutionPreparation>) {
        let Some(preparation) = preparation else {
            self.context_usage = 0.0;
            self.context_usage_detail = String::from("尚未生成预算快照");
            return;
        };

        if let Some(context_budget) = preparation.context_budget.as_ref() {
            self.context_usage = context_budget.usage_percent();
            self.context_usage_detail = context_budget.summary_note();
        } else {
            self.context_usage = 0.0;
            self.context_usage_detail = String::from("尚未生成预算快照");
        }
    }

    /// Resolves one approximate context-budget snapshot from the remembered session runtime identity and visible transcript history.
    /// 根据已记忆的会话运行时身份与可见转录历史解析一份近似上下文预算快照。
    fn resolve_session_context_budget_snapshot(
        &self,
        session_id: SessionId,
        runtime_state_snapshot: &SessionRuntimeStateEntry,
        _messages: &[SessionMessageEntry],
    ) -> Option<ContextBudgetSnapshot> {
        let (provider_id, model_id, api_family, capabilities) = if let (
            Some(provider_id),
            Some(model_id),
        ) = (
            runtime_state_snapshot.active_provider_id.as_deref(),
            runtime_state_snapshot.active_model_id.as_deref(),
        ) {
            let definition = provider_config::supported_provider_by_id(provider_id)?;
            let model =
                provider_config::effective_model_choices(provider_id, &self.provider_configuration)
                    .into_iter()
                    .find(|entry| entry.id == model_id);
            let capabilities = resolve_effective_model_capabilities(
                definition,
                model.as_ref(),
                &self.ai_tool_configuration,
            );
            (
                provider_id.to_string(),
                model_id.to_string(),
                definition.effective_api_family_for_model(Some(model_id)),
                capabilities,
            )
        } else {
            let active_choice = self.active_workbench_model_choice()?;
            let definition = provider_config::supported_provider_by_id(&active_choice.provider_id)?;
            let model = self.resolve_workbench_model_entry(&active_choice);
            let capabilities = resolve_effective_model_capabilities(
                definition,
                model.as_ref(),
                &self.ai_tool_configuration,
            );
            (
                active_choice.provider_id.clone(),
                active_choice.model_id.clone(),
                definition.effective_api_family_for_model(Some(&active_choice.model_id)),
                capabilities,
            )
        };
        let visible_tool_count = self
            .last_execution_preparation
            .as_ref()
            .filter(|preparation| {
                preparation.session_id == session_id
                    && preparation.model_blueprint.provider_id == provider_id
                    && preparation.model_blueprint.model_id == model_id
            })
            .map(|preparation| preparation.tool_descriptors.len())
            .unwrap_or(0usize);

        let mut core_snapshot =
            crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot(
                &runtime_state_snapshot.conversation_state,
            );
        core_snapshot.messages =
            crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history_for_model(
                &core_snapshot.messages,
                Some(api_family),
                Some(&capabilities),
            );

        Some(build_projected_session_context_budget_snapshot(
            &provider_id,
            &model_id,
            api_family,
            &capabilities,
            self.workbench_reasoning_effort,
            ContextCompactionPolicy::default(),
            &core_snapshot.messages,
            visible_tool_count,
            runtime_state_snapshot.last_provider_usage.as_ref(),
        ))
    }

    /// Synchronizes the visible workbench context bar from the latest session-scoped budget snapshot after compaction or session switching.
    /// 在压缩或会话切换后，根据最新会话级预算快照同步可见工作台上下文条。
    fn sync_context_usage_from_session_budget(&mut self, session_id: SessionId) {
        let selected_session_id = self.selected_session().map(|session| session.id);
        if selected_session_id != Some(session_id) {
            return;
        }

        let runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let messages = self.session_context_history(session_id);
        let context_budget = self.resolve_session_context_budget_snapshot(
            session_id,
            &runtime_state_snapshot,
            &messages,
        );

        if let Some(context_budget) = context_budget {
            self.context_usage = context_budget.usage_percent();
            self.context_usage_detail = context_budget.summary_note();
        }
    }

    /// Finalizes one streamed execution report, updates visible session state, and appends hidden derived nodes.
    /// 完成一份流式执行报告，更新可见会话状态并追加隐藏派生节点。
    fn apply_execution_run_report(&mut self, report: ExecutionRunReport) -> Task<Message> {
        let session_id = report.session_id;
        self.last_mcp_discovery_rows = report.mcp_discovery_rows.clone();
        self.last_mcp_discovery_errors = report.mcp_discovery_errors.clone();

        if let Some(preparation) = report.preparation.clone() {
            self.sync_context_usage_from_preparation(Some(&preparation));
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

        if report.error.is_some() {
            if let Some(controller_error) = report
                .controller_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, VulcanCoreEventKind::Error))
            {
                self.push_activity(
                    ActivityLevel::Warning,
                    "控制器轨迹错误",
                    format!("{}：{}", controller_error.title, controller_error.body),
                );
            }
        }

        for warning in &report.warnings {
            self.push_activity(ActivityLevel::Warning, "执行运行警告", warning.clone());
        }

        if let Some(next_conversation_state) = report.conversation_state.clone() {
            self.ensure_session_conversation_state_loaded(session_id);
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let preserved_records = runtime_state.conversation_state.records.clone();
            runtime_state.conversation_state = next_conversation_state;
            runtime_state.conversation_state.records = preserved_records;
            runtime_state.last_provider_usage =
                crate::provider_usage::latest_provider_usage_from_conversation_state(
                    &runtime_state.conversation_state,
                );
            runtime_state.updated_at = current_unix_millis_text();
            self.sync_session_messages_from_conversation_state(session_id);
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

        self.refresh_session_queued_steps(session_id);
        self.append_session_summary_output(session_id, &report);
        self.maybe_compact_session_history(session_id);
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
        self.refresh_subagent_navigation_round_visibility(session_id);

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            self.maybe_start_next_pending_prompt(session_id, false),
        ])
    }

    /// Creates one hidden compaction summary when the visible history grows beyond the current threshold.
    /// 当可见历史增长到当前阈值以上时创建一条隐藏压缩摘要。
    fn maybe_compact_session_history(&mut self, session_id: SessionId) {
        let messages = self.session_context_history(session_id);
        if messages.is_empty() {
            return;
        }
        let runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let Some(context_budget) = self.resolve_session_context_budget_snapshot(
            session_id,
            &runtime_state_snapshot,
            &messages,
        ) else {
            return;
        };
        if !context_budget.should_compact() {
            self.sync_context_usage_from_session_budget(session_id);
            return;
        }
        let Some(compaction_result) =
            self.session_skill_chain
                .run_compaction_skill(CompactionSkillInput {
                    session_id,
                    messages: &messages,
                    runtime_state: &runtime_state_snapshot,
                    policy: ContextCompactionPolicy::default(),
                })
        else {
            self.sync_context_usage_from_session_budget(session_id);
            return;
        };
        let summary_message_id =
            self.push_session_derived_record(session_id, compaction_result.summary_record.clone());
        let checkpoint = compaction_result.materialize_checkpoint(summary_message_id);
        let created_at = checkpoint.created_at.clone();
        let compaction_id = checkpoint.id.clone();
        let context_start_message_id = checkpoint.context_start_message_id.clone();
        let summary_message_id = checkpoint.summary_message_id.clone();
        self.session_compactions
            .entry(session_id)
            .or_default()
            .push(checkpoint);
        let runtime_state_updated_at = created_at.clone();
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state.last_compaction_id = Some(compaction_id);
            runtime_state.context_start_message_id = context_start_message_id;
            runtime_state.updated_at = created_at;
        }
        if let Some(summary_message_id) = summary_message_id {
            self.compact_session_conversation_history(
                session_id,
                &summary_message_id,
                &compaction_result.summary_record.body,
                &runtime_state_updated_at,
            );
        }
        self.push_activity(
            ActivityLevel::Info,
            "上下文预算触发压缩",
            format!(
                "会话 #{session_id} 已根据预算快照执行压缩：{}。",
                context_budget.summary_note()
            ),
        );
        self.sync_context_usage_from_session_budget(session_id);
    }

    /// Builds the current model-facing context history for one session from the start or from the latest compaction anchor.
    /// 从起点或最近一次压缩锚点开始为单个会话构建当前面向模型的上下文历史。
    fn session_context_history(&self, session_id: SessionId) -> Vec<SessionMessageEntry> {
        self.session_runtime_state(session_id)
            .map(|runtime_state| {
                conversation_state_to_context_records(
                    &runtime_state.conversation_state,
                    runtime_state.context_start_message_id.as_deref(),
                )
            })
            .unwrap_or_default()
    }

    /// Schedules one workspace-scoped BM25 history search for the left task panel.
    /// Schedules one workspace-scoped BM25 history search for the left task panel.
    /// 为左侧任务面板调度一次工作区级 BM25 历史搜索。
    fn schedule_workspace_history_search(&mut self, append: bool) -> Task<Message> {
        if self.workspace_history_search.loading {
            return Task::none();
        }
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };

        self.workspace_history_search.loading = true;
        self.workspace_history_search.loading_more = append;
        let query = self.workspace_history_search.query.clone();
        let project_scope = match &self.workspace_history_search.scope {
            WorkspaceHistorySearchScope::Global => None,
            WorkspaceHistorySearchScope::Project(project_id) => Some(project_id.clone()),
        };
        let offset = if append {
            self.workspace_history_search
                .results
                .as_ref()
                .map(|page| page.next_offset)
                .unwrap_or(0)
        } else {
            0
        };

        Task::perform(
            async move {
                workspace_store::search_workspace_history(
                    controller_runtime,
                    project_scope,
                    query,
                    SESSION_HISTORY_SEARCH_RESULT_LIMIT,
                    offset,
                )
                .await
            },
            move |result| Message::WorkspaceHistorySearchLoaded { append, result },
        )
    }

    /// Schedules best-effort storage maintenance for one already hydrated session.
    /// 为单个已水合会话调度尽力而为的存储维护任务。
    fn schedule_session_storage_maintenance(&self, session_id: SessionId) -> Task<Message> {
        if session_id.is_nil() {
            return Task::none();
        }
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };
        let messages = self
            .session_messages
            .get(&session_id)
            .or_else(|| self.archived_session_messages.get(&session_id))
            .map(|messages| {
                messages
                    .iter()
                    .map(|message| {
                        self.full_tool_detail_message_entry(session_id, &message.id)
                            .cloned()
                            .unwrap_or_else(|| message.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Task::perform(
            async move {
                let result = workspace_store::maintain_hydrated_session_storage(
                    controller_runtime,
                    session_id,
                    messages,
                )
                .await;
                (session_id, result)
            },
            |(session_id, result)| Message::SessionMaintenanceFinished(session_id, result),
        )
    }

    /// Schedules the next low-priority legacy session maintenance task if the queue is idle.
    /// 当队列空闲时调度下一条低优先级旧会话维护任务。
    fn schedule_next_background_legacy_session_maintenance(&mut self) -> Task<Message> {
        if self.background_legacy_maintenance_in_flight.is_some() {
            return Task::none();
        }
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };

        while let Some(session_id) = self.background_legacy_maintenance_queue.pop_front() {
            if session_id.is_nil() {
                continue;
            }
            self.background_legacy_maintenance_in_flight = Some(session_id);
            return Task::perform(
                async move {
                    let result = workspace_store::maintain_legacy_session_storage(
                        controller_runtime,
                        session_id,
                    )
                    .await;
                    (session_id, result)
                },
                |(session_id, result)| {
                    Message::BackgroundLegacySessionMaintenanceFinished(session_id, result)
                },
            );
        }

        Task::none()
    }

    /// Rebuilds the background legacy maintenance queue for the currently loaded workspace.
    /// 为当前已加载工作台重建后台旧数据维护队列。
    fn reset_background_legacy_session_maintenance_queue(&mut self) {
        self.background_legacy_maintenance_queue =
            build_background_legacy_maintenance_queue(&self.sessions, &self.archived_sessions);
        self.background_legacy_maintenance_in_flight = None;
        self.background_legacy_maintenance_completed_count = 0;
        self.background_legacy_maintenance_failed_count = 0;
    }

    /// Removes one session from any background legacy maintenance bookkeeping.
    /// 从后台旧数据维护状态中移除单个会话。
    fn remove_session_from_background_legacy_maintenance(&mut self, session_id: SessionId) {
        self.background_legacy_maintenance_queue
            .retain(|queued_session_id| *queued_session_id != session_id);
        if self.background_legacy_maintenance_in_flight == Some(session_id) {
            self.background_legacy_maintenance_in_flight = None;
        }
    }

    /// Records a user-visible activity entry when a maintenance pass migrated legacy chunks.
    /// 当维护过程迁移了旧版分片时记录一条用户可见活动日志。
    fn record_session_legacy_chunk_migration_activity(
        &mut self,
        title: &'static str,
        report: &SessionMaintenanceReport,
    ) {
        if report.legacy_chunk_migration.migrated_message_count == 0 {
            return;
        }

        self.push_activity(
            ActivityLevel::Success,
            title,
            format!(
                "session_id={}，已迁移 {} 条旧版内联消息，生成约 {} 个正文分片。",
                report.legacy_chunk_migration.session_id,
                report.legacy_chunk_migration.migrated_message_count,
                report.legacy_chunk_migration.inserted_chunk_count
            ),
        );
    }

    /// Prunes one hydrated session's in-memory transcript cache without deleting persisted history.
    /// 在不删除持久化历史的前提下裁剪单个已水合会话的内存转录缓存。
    fn prune_session_message_cache_to_limit(&mut self, session_id: SessionId) -> usize {
        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let removed_message_ids = if session_is_archived {
            self.archived_session_messages
                .get_mut(&session_id)
                .map(|messages| {
                    prune_session_messages_to_limit(messages, SESSION_IN_MEMORY_MESSAGE_LIMIT)
                })
                .unwrap_or_default()
        } else {
            self.session_messages
                .get_mut(&session_id)
                .map(|messages| {
                    prune_session_messages_to_limit(messages, SESSION_IN_MEMORY_MESSAGE_LIMIT)
                })
                .unwrap_or_default()
        };
        if removed_message_ids.is_empty() {
            return 0;
        }

        self.clear_session_message_view_caches(&removed_message_ids);
        let remove_tool_detail_bucket =
            if let Some(tool_details) = self.session_tool_detail_messages.get_mut(&session_id) {
                for message_id in &removed_message_ids {
                    tool_details.remove(message_id);
                }
                tool_details.is_empty()
            } else {
                false
            };
        if remove_tool_detail_bucket {
            self.session_tool_detail_messages.remove(&session_id);
        }
        self.markdown_session_message_ids.remove(&session_id);
        self.session_history_has_more.remove(&session_id);
        self.session_history_earliest_ordinals.remove(&session_id);
        self.loading_older_session_history_ids.remove(&session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_agent_view_selectable_documents
            .remove(&session_id);
        self.session_agent_view_documents.remove(&session_id);
        self.session_inline_agent_view_documents.remove(&session_id);
        self.session_agent_view_block_collapsed.remove(&session_id);
        self.close_thinking_modal_for_session(session_id);
        self.close_tool_modal_for_session(session_id);

        removed_message_ids.len()
    }

    /// Applies one persisted workspace snapshot to the live GUI state in a single place.
    /// 将一份持久化工作台快照集中应用到当前图形界面状态。
    fn apply_workspace_snapshot(&mut self, snapshot: WorkspaceSnapshot) {
        self.task_projects = snapshot
            .task_projects
            .into_iter()
            .map(|mut project| {
                project.path = normalize_workspace_path_text(&project.path);
                project
            })
            .collect();
        self.archived_task_projects = snapshot
            .archived_task_projects
            .into_iter()
            .map(|mut project| {
                project.path = normalize_workspace_path_text(&project.path);
                project
            })
            .collect();
        self.sessions = snapshot.sessions;
        self.archived_sessions = snapshot.archived_sessions;
        self.session_messages.clear();
        self.session_tool_detail_messages.clear();
        self.archived_session_messages.clear();
        self.session_compactions = snapshot.session_compactions;
        self.archived_session_compactions = snapshot.archived_session_compactions;
        self.session_runtime_state = snapshot.session_runtime_state;
        self.archived_session_runtime_state = snapshot.archived_session_runtime_state;
        self.hydrated_session_ids = snapshot.hydrated_session_ids;
        self.dirty_session_payload_ids.clear();
        self.dirty_session_message_ids.clear();
        self.loading_session_ids.clear();
        self.loading_session_agent_view_projection_ids.clear();
        self.session_history_earliest_ordinals.clear();
        self.session_history_has_more.clear();
        self.loading_older_session_history_ids.clear();
        self.workspace_history_search = WorkspaceHistorySearchPanelState::default();
        self.reset_background_legacy_session_maintenance_queue();
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
        self.active_tool_modal = None;
        self.session_subagent_tasks.clear();
        self.session_subagent_navigation_rounds.clear();
        self.active_subagent_panel = None;
        self.subagent_navigation_expanded = false;
        self.detached_subagent_task_listeners.clear();
        self.streaming_output_message_ids.clear();
        self.rebuild_session_message_raw_buffers();
        self.rebuild_session_message_markdown();
        self.runtime_event_raw_buffers.clear();
        self.runtime_event_markdown.clear();
        self.runtime_event_markdown_buffers.clear();
        self.session_agent_view_buffers.clear();
        self.session_agent_view_raw_buffers.clear();
        self.session_agent_view_selectable_documents.clear();
        self.session_agent_view_documents.clear();
        self.session_inline_agent_view_documents.clear();
        self.session_agent_view_block_collapsed.clear();
        self.session_agent_view_follow_tail.clear();
        self.session_agent_view_viewports.clear();
        self.pending_transcript_follow_tail_session_ids.clear();
        self.session_streaming_thinking_viewports.clear();
        self.pending_thinking_overlay_follow_tail_session_ids
            .clear();
        self.sync_workspace_path_with_selected_session();
        self.workspace_path = normalize_workspace_path_text(&self.workspace_path);
        self.refresh_all_session_queued_steps();
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
        self.pending_transcript_follow_tail_session_ids
            .remove(&session_id);
        self.pending_thinking_overlay_follow_tail_session_ids
            .remove(&session_id);
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
        self.session_tool_detail_messages.remove(&session_id);
        self.loading_session_agent_view_projection_ids
            .remove(&session_id);
        self.collapsed_runtime_event_ids.remove(&session_id);
        self.markdown_runtime_event_ids.remove(&session_id);
        self.remove_runtime_event_raw_buffers(session_id);
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_last_api_request_buffers.remove(&session_id);
        self.session_agent_view_documents.remove(&session_id);
        self.session_inline_agent_view_documents.remove(&session_id);
        self.session_agent_view_block_collapsed.remove(&session_id);
        self.session_agent_view_follow_tail.remove(&session_id);
        self.session_agent_view_viewports.remove(&session_id);
        self.pending_transcript_follow_tail_session_ids
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.session_streaming_thinking_viewports
            .remove(&session_id);
        self.pending_thinking_overlay_follow_tail_session_ids
            .remove(&session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.close_thinking_modal_for_session(session_id);
        self.close_tool_modal_for_session(session_id);
        self.session_subagent_tasks.remove(&session_id);
        self.session_subagent_navigation_rounds.remove(&session_id);
        if self
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            self.active_subagent_panel = None;
        }
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
    fn persist_workspace_snapshot_task(&mut self) -> Task<Message> {
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };
        if self.workspace_snapshot_persist_in_flight {
            self.workspace_snapshot_persist_pending = true;
            return Task::none();
        }

        self.workspace_snapshot_persist_in_flight = true;
        self.workspace_snapshot_persist_pending = false;
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
                let message_id = self.push_visible_session_record_message_with_metadata(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ToolCallOutput,
                    title,
                    event.detail.clone(),
                    event.metadata.clone(),
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
                let result_title = self
                    .session_tool_call_message_ids
                    .get(&session_id)
                    .and_then(|entries| entries.get(&tool_call_id))
                    .and_then(|message_id| {
                        self.session_messages.get(&session_id).and_then(|messages| {
                            messages
                                .iter()
                                .find(|message| message.id == *message_id)
                                .map(|message| {
                                    let call_title = message.title.trim();
                                    if call_title.is_empty() {
                                        format!("调用 {tool_call_id} 工具")
                                    } else {
                                        call_title.to_string()
                                    }
                                })
                        })
                    })
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| format!("调用 {tool_call_id} 工具"));
                self.push_visible_session_record_message_with_metadata(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ToolResultOutput,
                    result_title,
                    event.detail.clone(),
                    event.metadata.clone(),
                    false,
                );

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
        if self.ingest_subagent_runtime_event(session_id, &event) {
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

    /// Consumes one delegate runtime event into the dedicated subagent panel cache and returns whether the event should stop there.
    /// 将一条委派运行时事件吸收到专用子代理面板缓存中，并返回该事件是否应在此停止传播。
    fn ingest_subagent_runtime_event(
        &mut self,
        session_id: SessionId,
        event: &RuntimeEvent,
    ) -> bool {
        if !matches!(
            event.kind,
            RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished
        ) {
            return false;
        }
        let is_finished = event.kind == RuntimeEventKind::DelegateFinished;

        let Some(metadata) = event.metadata.as_ref() else {
            return true;
        };
        self.attach_task_session_metadata_to_parent_tool_message(session_id, metadata);
        let prompt = parse_subagent_task_prompt_metadata(metadata);
        let Some(state) = parse_subagent_task_panel_metadata(session_id, metadata) else {
            return true;
        };
        self.session_subagent_navigation_rounds.insert(session_id);
        if let Some(prompt) = prompt.as_deref() {
            self.seed_subagent_task_prompt_if_needed(state.task_id, prompt);
        }
        if let Some(conversation_state) = state.conversation_state.clone() {
            self.replace_subagent_task_conversation_state(state.task_id, conversation_state);
        }
        if is_finished {
            self.detached_subagent_task_listeners.remove(&state.task_id);
            self.enqueue_delegate_completion_followup_if_needed(session_id, &state);
        }
        self.upsert_subagent_task_panel_state(state);
        if is_finished {
            self.refresh_subagent_navigation_round_visibility(session_id);
        }
        true
    }

    /// Updates the execution phase tracked for one session transcript.
    /// 更新某个会话转录区跟踪的执行阶段。
    fn set_session_run_phase(&mut self, session_id: SessionId, phase: SessionRunPhase) {
        self.session_run_phases.insert(session_id, phase);
    }

    /// Returns whether one specific session currently owns an active real-AI run.
    /// 返回某个指定会话当前是否拥有活动的真实 AI 运行。
    fn session_has_active_run(&self, session_id: SessionId) -> bool {
        self.active_runs.contains_key(&session_id)
    }

    /// Clears the active run state tracked for one session after its stream finishes or is stopped.
    /// 在某个会话的流结束或被停止后清理其活动运行状态。
    fn clear_active_run_tracking(&mut self, session_id: SessionId) {
        self.active_runs.remove(&session_id);
    }

    /// Removes and returns the active run state tracked for one session.
    /// 移除并返回某个会话当前跟踪的活动运行状态。
    fn take_active_run_state(&mut self, session_id: SessionId) -> Option<ActiveSessionRunState> {
        self.active_runs.remove(&session_id)
    }

    /// Returns the session identifier that owns the provided run identifier, if it is still active.
    /// 返回持有指定运行标识的会话标识（如果该运行仍然处于活动状态）。
    fn active_run_session_id_for_run_id(&self, run_id: u64) -> Option<SessionId> {
        self.active_runs
            .iter()
            .find_map(|(session_id, active_run)| {
                if active_run.run_id == run_id {
                    Some(*session_id)
                } else {
                    None
                }
            })
    }

    /// Returns whether the provided generated-media local path currently matches the active inline audio playback entry.
    /// 返回给定生成媒体本地路径当前是否匹配活动中的内联音频播放条目。
    fn is_generated_audio_playing_local_path(&self, local_path: &str) -> bool {
        self.generated_audio_playback
            .as_ref()
            .is_some_and(|playback| playback.local_path == local_path && !playback.sink.empty())
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
        if self.active_transcript_surface_session_id() != Some(session_id) {
            return Task::none();
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(transcript_scrollable_id(session_id))
    }

    /// Records one streaming transcript tail-follow request so repeated token updates share one timed scroll operation.
    /// 记录一次流式转录尾部追随请求，使重复 token 更新共享一次定时滚动操作。
    fn request_selected_session_transcript_tail_follow(&mut self, session_id: SessionId) {
        if self.active_transcript_surface_session_id() != Some(session_id) {
            return;
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return;
        }

        self.pending_transcript_follow_tail_session_ids
            .insert(session_id);
    }

    /// Applies one coalesced transcript tail-follow request for the currently visible transcript surface.
    /// 为当前可见转录表面执行一次已合并的尾部追随请求。
    fn perform_pending_transcript_tail_follow_tick(&mut self) -> Task<Message> {
        let pending_session_ids =
            std::mem::take(&mut self.pending_transcript_follow_tail_session_ids);
        let Some(session_id) = self.active_transcript_surface_session_id() else {
            return Task::none();
        };

        if !pending_session_ids.contains(&session_id) {
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
        if self.active_transcript_surface_session_id() != Some(session_id) {
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

        if self.active_transcript_surface_session_id() != Some(active_state.session_id) {
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

    /// Records one streaming thinking-overlay tail-follow request so repeated token updates share one timed scroll operation.
    /// 记录一次流式思考浮层尾部追随请求，使重复 token 更新共享一次定时滚动操作。
    fn request_selected_session_thinking_overlay_tail_follow(&mut self, session_id: SessionId) {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return;
        }

        if self.streaming_thinking_message(session_id).is_none() {
            return;
        }

        if !self.session_streaming_thinking_should_follow_tail(session_id) {
            return;
        }

        self.pending_thinking_overlay_follow_tail_session_ids
            .insert(session_id);
    }

    /// Applies one coalesced floating thinking-overlay tail-follow request for the currently selected session.
    /// 为当前选中会话执行一次已合并的流式思考浮层尾部追随请求。
    fn perform_pending_thinking_overlay_tail_follow_tick(&mut self) -> Task<Message> {
        let pending_session_ids =
            std::mem::take(&mut self.pending_thinking_overlay_follow_tail_session_ids);
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return Task::none();
        };

        if !pending_session_ids.contains(&session_id) {
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

    /// Opens the batch target-picker modal for one semantic AI capability by preloading the current enabled selections from configured vendors.
    /// 通过从已配置供应商中预载当前启用选择，打开某个语义 AI 能力的批量目标选择弹窗。
    fn open_ai_capability_target_picker(&mut self, capability_kind: AiToolKind) -> Task<Message> {
        let vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();
        if vendor_ids.is_empty() {
            self.ai_tool_notice = format!(
                "当前还没有可用于“{}”的共享供应商，请先到“能力供应商”页新增并填写凭据。",
                capability_kind.label()
            );
            self.status_message = self.ai_tool_notice.clone();
            self.selected_settings_section = SettingsSection::AiCapabilityVendors;
            self.settings_ai_capability_tree_expanded = true;
            return Task::none();
        }

        let mut selections = BTreeMap::new();
        for vendor_id in &vendor_ids {
            let Some(vendor) = self.ai_tool_configuration.vendor_by_id(vendor_id) else {
                continue;
            };
            let selected_targets = vendor
                .targets
                .iter()
                .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
                .map(|binding| {
                    (
                        binding.target_alias().to_string(),
                        binding.model_id().to_string(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            selections.insert(vendor_id.clone(), selected_targets);
        }

        self.ai_capability_target_picker = Some(AiCapabilityTargetPickerState {
            capability_kind,
            selected_vendor_id: vendor_ids.first().cloned(),
            selections,
        });
        Task::none()
    }

    /// Updates one staged target checkbox inside the currently visible AI capability picker without mutating persisted configuration yet.
    /// 在当前可见 AI 能力选择器中更新某个暂存目标勾选状态，但暂不修改持久化配置。
    fn stage_ai_capability_target_picker_toggle(
        &mut self,
        vendor_id: &str,
        target_alias: &str,
        enabled: bool,
    ) {
        let Some(picker) = self.ai_capability_target_picker.as_mut() else {
            return;
        };
        let choices = provider_config::ai_capability_vendor_target_choices(vendor_id);
        let Some(choice) = choices.into_iter().find(|choice| {
            choice.capability_kind == picker.capability_kind
                && choice.target_alias.eq_ignore_ascii_case(target_alias)
        }) else {
            return;
        };
        if enabled {
            let current_selected_count = picker
                .selections
                .values()
                .map(std::collections::BTreeMap::len)
                .sum::<usize>();
            let already_selected = picker
                .selections
                .get(&vendor_id.to_ascii_lowercase())
                .is_some_and(|targets| targets.contains_key(choice.target_alias.as_str()));
            if !already_selected
                && current_selected_count
                    >= provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
            {
                self.ai_tool_notice = format!(
                    "“{}”最多只能启用 {} 个接口，请先取消部分已勾选项。",
                    picker.capability_kind.label(),
                    provider_config::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
                );
                self.status_message = self.ai_tool_notice.clone();
                return;
            }
            let vendor_entry = picker
                .selections
                .entry(vendor_id.to_ascii_lowercase())
                .or_default();
            vendor_entry
                .entry(choice.target_alias.clone())
                .or_insert(choice.default_model_id);
        } else {
            if let Some(vendor_entry) = picker.selections.get_mut(&vendor_id.to_ascii_lowercase()) {
                vendor_entry.remove(choice.target_alias.as_str());
            }
        }
    }

    /// Updates one staged request model inside the currently visible AI capability picker.
    /// 更新当前可见 AI 能力选择器中的某个暂存请求模型。
    fn stage_ai_capability_target_picker_model(
        &mut self,
        vendor_id: &str,
        target_alias: &str,
        model_id: String,
    ) {
        let Some(picker) = self.ai_capability_target_picker.as_mut() else {
            return;
        };
        let vendor_entry = picker
            .selections
            .entry(vendor_id.to_ascii_lowercase())
            .or_default();
        if let Some(current) = vendor_entry.get_mut(target_alias) {
            *current = model_id;
        }
    }

    /// Applies the staged target selections from the picker modal back into the shared vendor-centric AI capability configuration.
    /// 将选择器弹窗中的暂存目标选择应用回共享供应商中心 AI 能力配置。
    fn apply_ai_capability_target_picker_selections(&mut self) -> Task<Message> {
        let Some(picker) = self.ai_capability_target_picker.take() else {
            return Task::none();
        };

        let vendor_ids = self
            .ai_tool_configuration
            .vendors
            .iter()
            .filter(|vendor| {
                provider_config::ai_capability_vendor_target_choices(&vendor.vendor_id)
                    .into_iter()
                    .any(|choice| choice.capability_kind == picker.capability_kind)
            })
            .map(|vendor| vendor.vendor_id.clone())
            .collect::<Vec<_>>();

        for vendor_id in vendor_ids {
            let selected_targets = picker
                .selections
                .get(&vendor_id)
                .map(|targets| {
                    targets
                        .iter()
                        .map(|(target_alias, model_id)| (target_alias.clone(), model_id.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.replace_ai_capability_vendor_targets_for_kind(
                &vendor_id,
                picker.capability_kind,
                selected_targets,
            );
        }

        self.persist_ai_tool_configuration_change(format!(
            "已更新“{}”的启用接口并自动保存。",
            picker.capability_kind.label()
        ));
        Task::none()
    }

    /// Opens the shared AI capability vendor editor either for one brand-new vendor or for one already configured vendor instance.
    /// 为全新供应商或已有供应商实例打开共享 AI 能力供应商编辑器。
    fn open_ai_capability_vendor_editor(&mut self, vendor_id: &str, is_new: bool) {
        let normalized_vendor_id = vendor_id.trim().to_ascii_lowercase();
        let existing_vendor = self
            .ai_tool_configuration
            .vendor_by_id(&normalized_vendor_id);
        let display_name =
            provider_config::ai_capability_vendor_display_name(&normalized_vendor_id);
        let region_options =
            crate::ai_capability_provider_catalog::ai_capability_vendor_region_options(
                &normalized_vendor_id,
            );
        let connection_mode = existing_vendor
            .map(|vendor| vendor.resolved_connection_mode())
            .unwrap_or(crate::provider_config::AiCapabilityVendorConnectionMode::Local);
        let region = existing_vendor
            .map(|vendor| vendor.region.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| region_options.first().cloned())
            .unwrap_or_default();
        self.ai_capability_vendor_editor = Some(AiCapabilityVendorEditorState {
            vendor_id: normalized_vendor_id,
            display_name,
            api_key: existing_vendor
                .map(|vendor| vendor.api_key.clone())
                .unwrap_or_default(),
            connection_mode,
            base_url: existing_vendor
                .map(|vendor| vendor.base_url.clone())
                .unwrap_or_default(),
            region,
            region_options,
            is_new,
        });
    }

    /// Persists the staged shared AI capability vendor editor back into the vendor list, creating the vendor first when needed.
    /// 将暂存的共享 AI 能力供应商编辑器内容持久化回供应商列表，并在需要时先创建该供应商。
    fn save_ai_capability_vendor_editor(&mut self) -> Task<Message> {
        let Some(editor) = self.ai_capability_vendor_editor.take() else {
            return Task::none();
        };
        let definitions =
            crate::ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(
                &editor.vendor_id,
            );
        let supports_api_key_field = definitions.iter().any(|definition| {
            !matches!(
                definition.api_key_requirement,
                crate::ai_capability_provider_catalog::AiCapabilityProviderApiKeyRequirement::None
            )
        });
        let supports_region_field = definitions
            .iter()
            .any(|definition| definition.supports_region_field)
            || !editor.region_options.is_empty();
        if editor.vendor_id.eq_ignore_ascii_case("ollama")
            && matches!(
                editor.connection_mode,
                crate::provider_config::AiCapabilityVendorConnectionMode::Cloud
            )
            && editor.api_key.trim().is_empty()
        {
            self.ai_tool_notice = String::from(
                "Ollama Cloud 模式需要填写 API Key；若要连接本地 Ollama，请切换到“本地”模式。",
            );
            self.status_message = self.ai_tool_notice.clone();
            self.ai_capability_vendor_editor = Some(editor);
            return Task::none();
        }
        if editor.is_new && !self.add_ai_capability_vendor(&editor.vendor_id) {
            self.ai_tool_notice = format!("共享能力供应商 `{}` 已存在。", editor.display_name);
            self.status_message = self.ai_tool_notice.clone();
            return Task::none();
        }
        let vendor_id = editor.vendor_id.clone();
        let display_name = editor.display_name.clone();
        self.update_ai_capability_vendor(&vendor_id, |vendor| {
            vendor.name = provider_config::ai_capability_vendor_display_name(&vendor_id);
            vendor.enabled = true;
            vendor.api_key = if supports_api_key_field {
                editor.api_key.trim().to_string()
            } else {
                String::new()
            };
            vendor.connection_mode = if vendor_id.eq_ignore_ascii_case("ollama") {
                Some(editor.connection_mode)
            } else {
                None
            };
            vendor.base_url = if vendor_id.eq_ignore_ascii_case("ollama")
                && matches!(
                    editor.connection_mode,
                    crate::provider_config::AiCapabilityVendorConnectionMode::Local
                ) {
                editor.base_url.trim().to_string()
            } else {
                String::new()
            };
            vendor.region = if supports_region_field {
                editor.region.trim().to_string()
            } else {
                String::new()
            };
        });
        let success_notice = if editor.is_new {
            format!("已新增共享能力供应商 `{display_name}` 并自动保存。")
        } else {
            format!("已更新共享能力供应商 `{display_name}` 并自动保存。")
        };
        self.persist_ai_tool_configuration_change(success_notice);
        Task::none()
    }

    /// Persists the current standalone AI capability configuration immediately after one successful in-memory mutation and updates the shared status notices.
    /// 在一次成功的内存配置变更之后立即持久化当前独立 AI 能力配置，并更新共享状态提示。
    fn persist_ai_tool_configuration_change(&mut self, success_notice: String) {
        match self.save_ai_tool_configuration() {
            Ok(()) => {
                self.ai_tool_notice = success_notice;
                self.status_message = self.ai_tool_notice.clone();
            }
            Err(error) => {
                self.ai_tool_notice = error.clone();
                self.status_message = error;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        anthropic_pending_prompt_dispatch_is_blocked, build_background_legacy_maintenance_queue,
        compact_tool_detail_message_for_projection, composer_text_has_submittable_content,
        composer_visible_line_count, generated_attachments_from_tool_metadata,
        merge_projected_session_messages_into_existing_window, merge_tool_display_messages,
        pending_prompt_dispatch_index, prune_session_messages_to_limit,
        task_session_payload_from_tool_metadata,
    };
    use crate::model::{
        PendingPromptRuntimeOptions, PendingSessionPromptEntry, PendingSessionPromptSource,
        SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionState,
        SessionSummary,
    };
    use iced::widget::text_editor;
    use serde_json::{Value, json};
    use vulcan_core::types::ConversationAttachmentKind;

    /// Builds a compact session summary for queue-related unit tests.
    /// 为队列相关单元测试构建一条紧凑会话摘要。
    fn test_session_summary(id: SessionId, title: &str) -> SessionSummary {
        SessionSummary::new(
            id,
            None,
            title.to_string(),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        )
    }

    /// Builds one compact pending-session prompt entry for queue-order unit tests.
    /// 为队列顺序相关单元测试构建一条紧凑的待发送提示词项。
    fn test_pending_prompt_entry(
        source: PendingSessionPromptSource,
        prompt: &str,
    ) -> PendingSessionPromptEntry {
        PendingSessionPromptEntry {
            id: format!("entry-{prompt}"),
            prompt: prompt.to_string(),
            preview: prompt.to_string(),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source,
            task_id: (source == PendingSessionPromptSource::DelegateCompletion)
                .then(|| format!("task-{prompt}")),
            panel_entry_id: None,
            created_at: String::from("1"),
        }
    }

    /// Verifies that an empty composer still reserves one visible line.
    /// 验证空提示词编辑器仍会保留一条可见逻辑行。
    #[test]
    fn composer_visible_line_count_keeps_one_line_for_empty_content() {
        let buffer = text_editor::Content::with_text("");

        assert_eq!(composer_visible_line_count(&buffer, 420.0), 1);
    }

    /// Verifies that a trailing newline immediately expands the composer to the next logical line.
    /// 验证尾部换行会立刻把提示词编辑器扩展到下一条逻辑行。
    #[test]
    fn composer_visible_line_count_preserves_trailing_blank_line() {
        let buffer = text_editor::Content::with_text("第一行\n");

        assert_eq!(composer_visible_line_count(&buffer, 420.0), 2);
    }

    /// Verifies that one long logical line expands into multiple visible lines when the composer width becomes narrow enough.
    /// 验证当提示词编辑器宽度变窄时，单条超长逻辑行会自动扩展为多条可见行。
    #[test]
    fn composer_visible_line_count_expands_for_soft_wrapped_content() {
        let buffer = text_editor::Content::with_text(
            "This is a long composer line that should wrap automatically inside a narrow surface.",
        );

        assert!(composer_visible_line_count(&buffer, 180.0) > 1);
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

    /// Verifies that the background legacy-maintenance queue includes visible and archived sessions once.
    /// 验证后台旧数据维护队列会将可见与归档会话各纳入一次。
    #[test]
    fn background_legacy_maintenance_queue_deduplicates_sessions() {
        let first_session_id = SessionId::from_u128(101);
        let second_session_id = SessionId::from_u128(102);
        let visible_sessions = vec![test_session_summary(first_session_id, "可见会话")];
        let archived_sessions = vec![
            test_session_summary(first_session_id, "重复归档会话"),
            test_session_summary(second_session_id, "归档会话"),
        ];

        let queue =
            build_background_legacy_maintenance_queue(&visible_sessions, &archived_sessions);

        assert_eq!(
            queue.into_iter().collect::<Vec<_>>(),
            vec![first_session_id, second_session_id]
        );
    }

    /// Verifies that the background legacy-maintenance queue ignores nil draft identifiers.
    /// 验证后台旧数据维护队列会忽略空的草稿标识。
    #[test]
    fn background_legacy_maintenance_queue_skips_nil_sessions() {
        let session_id = SessionId::from_u128(103);
        let visible_sessions = vec![
            test_session_summary(SessionId::nil(), "空会话"),
            test_session_summary(session_id, "正常会话"),
        ];

        let queue = build_background_legacy_maintenance_queue(&visible_sessions, &[]);

        assert_eq!(queue.into_iter().collect::<Vec<_>>(), vec![session_id]);
    }

    /// Verifies Anthropic queue dispatch prefers delegated completion callbacks over user-authored follow-ups once background subagents have fully settled.
    /// 验证在后台子代理全部收敛后，Anthropic 队列分发会优先选择子代理完成回流项，而不是优先处理用户跟进消息。
    #[test]
    fn pending_prompt_dispatch_index_prefers_delegate_completion_for_anthropic() {
        let queue = vec![
            test_pending_prompt_entry(PendingSessionPromptSource::UserInterrupt, "interrupt"),
            test_pending_prompt_entry(
                PendingSessionPromptSource::DelegateCompletion,
                "delegate-finished",
            ),
            test_pending_prompt_entry(PendingSessionPromptSource::UserFollowup, "followup"),
        ];

        assert_eq!(pending_prompt_dispatch_index(&queue, true), 1);
        assert_eq!(pending_prompt_dispatch_index(&queue, false), 0);
    }

    /// Verifies Anthropic sessions block pending-prompt dispatch while detached subagents from the previous tool round are still running, whereas other runtimes keep the original behavior.
    /// 验证当上一轮工具回合的分离子代理仍在运行时，Anthropic 会话会阻止待发送提示词分发，而其他运行时保持原有行为。
    #[test]
    fn anthropic_pending_prompt_dispatch_blocks_only_for_running_subagents() {
        assert!(anthropic_pending_prompt_dispatch_is_blocked(true, true));
        assert!(!anthropic_pending_prompt_dispatch_is_blocked(true, false));
        assert!(!anthropic_pending_prompt_dispatch_is_blocked(false, true));
        assert!(!anthropic_pending_prompt_dispatch_is_blocked(false, false));
    }

    /// Verifies that transcript pruning removes the oldest messages first.
    /// 验证转录裁剪会优先移除最旧的消息。
    #[test]
    fn prune_session_messages_to_limit_removes_oldest_entries() {
        let mut messages = (0..5)
            .map(|index| SessionMessageEntry {
                id: format!("message-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: format!("消息 {index}"),
                body: format!("正文 {index}"),
                metadata: None,
                created_at: index.to_string(),
            })
            .collect::<Vec<_>>();

        let removed = prune_session_messages_to_limit(&mut messages, 3);

        assert_eq!(removed, vec!["message-0", "message-1"]);
        assert_eq!(
            messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["message-2", "message-3", "message-4"]
        );
    }

    /// Verifies that transcript pruning is a no-op when the current window is already small enough.
    /// 验证当前窗口足够小时转录裁剪不会产生任何修改。
    #[test]
    fn prune_session_messages_to_limit_keeps_small_windows_unchanged() {
        let mut messages = vec![SessionMessageEntry::new(
            SessionMessageKind::User,
            "用户指令",
            "继续",
        )];

        let removed = prune_session_messages_to_limit(&mut messages, 3);

        assert!(removed.is_empty());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].title, "用户指令");
    }

    /// Verifies one tool call and its matching tool result merge into a single human-facing transcript card.
    /// 验证一条工具调用与其匹配的工具结果会合并成面向人类的单张转录卡片。
    #[test]
    fn merge_tool_display_messages_collapses_matching_tool_rows() {
        let messages = vec![
            SessionMessageEntry {
                id: String::from("tool-call-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 read_file 工具"),
                body: String::from("## 调用参数\n\n```json\n{\"path\":\"src/main.rs\"}\n```"),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "tool_name": "read_file",
                    "arguments": {
                        "path": "src/main.rs",
                    },
                })),
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("tool-result-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回 read_file"),
                body: String::from("## 工具返回\n\n成功读取文件。"),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "tool_result_id": "result-1",
                    "host_result": {
                        "kind": "change_set",
                        "payload": {
                            "path": "src/main.rs",
                        },
                    },
                })),
                created_at: String::from("2"),
            },
        ];

        let merged = merge_tool_display_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "tool-call-1");
        assert_eq!(
            merged[0].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert_eq!(merged[0].title, "调用 read_file 工具");
        assert!(merged[0].body.contains("## 调用参数"));
        assert!(merged[0].body.contains("## 工具返回"));
        assert_eq!(
            merged[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_result_id"))
                .and_then(|value| value.as_str()),
            Some("result-1")
        );
        assert_eq!(
            merged[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    /// Verifies an in-flight tool call still stays visible on its own when no matching result has arrived yet.
    /// 验证当尚未收到匹配结果时，进行中的工具调用仍会单独保持可见。
    #[test]
    fn merge_tool_display_messages_keeps_inflight_tool_call_visible() {
        let messages = vec![SessionMessageEntry {
            id: String::from("tool-call-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::from("调用 read_file 工具"),
            body: String::from("## 调用参数\n\n```json\n{\"path\":\"src/main.rs\"}\n```"),
            metadata: Some(json!({
                "tool_call_id": "call-1",
                "tool_name": "read_file",
            })),
            created_at: String::from("1"),
        }];

        let merged = merge_tool_display_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].record_kind,
            SessionMessageRecordKind::ToolCallOutput
        );
        assert_eq!(merged[0].title, "调用 read_file 工具");
        assert_eq!(
            merged[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(|value| value.as_str()),
            Some("running")
        );
    }

    /// Verifies syncing visible transcript rows from the canonical conversation state preserves already hydrated older history rows that are not part of the live runtime tail.
    /// 验证从规范 conversation state 同步可见转录行时，会保留已水合但不属于当前运行时尾部的更早历史行。
    #[test]
    fn merge_projected_session_messages_preserves_hydrated_history_prefix() {
        let existing_messages = vec![
            SessionMessageEntry {
                id: String::from("older-tool-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用历史工具"),
                body: String::new(),
                metadata: Some(json!({
                    "tool_call_id": "older-call-1",
                    "tool_display_state": "completed",
                })),
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("assistant-tail-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("旧尾部"),
                metadata: None,
                created_at: String::from("2"),
            },
        ];
        let projected_messages = vec![
            SessionMessageEntry {
                id: String::from("assistant-tail-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("新尾部"),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionMessageEntry {
                id: String::from("assistant-tail-2"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("最新输出"),
                metadata: None,
                created_at: String::from("3"),
            },
        ];

        let merged = merge_projected_session_messages_into_existing_window(
            &existing_messages,
            projected_messages,
        );

        assert_eq!(
            merged
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["older-tool-1", "assistant-tail-1", "assistant-tail-2"]
        );
        assert_eq!(merged[1].body, "新尾部");
        assert_eq!(merged[2].body, "最新输出");
    }

    /// Verifies that main-transcript tool projections drop heavy bodies and retain only card metadata.
    /// 验证主转录工具投影会移除重型正文，并仅保留卡片所需元数据。
    #[test]
    fn compact_tool_detail_projection_drops_hidden_payloads() {
        let message = SessionMessageEntry {
            id: String::from("tool-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 read_file 工具"),
            body: "x".repeat(200_000),
            metadata: Some(json!({
                "tool_call_id": "call-1",
                "tool_display_state": "completed",
                "host_result": {
                    "kind": "raw_payload",
                    "payload": "y".repeat(200_000)
                },
                "payload": {
                    "task_id": "task-a",
                    "prompt": "z".repeat(1000),
                    "conversation_state": {"large": "z".repeat(200_000)}
                }
            })),
            created_at: String::from("1"),
        };

        let projected = compact_tool_detail_message_for_projection(&message);

        assert!(projected.body.is_empty());
        let metadata = projected.metadata.expect("metadata should stay present");
        assert_eq!(
            metadata.get("tool_display_state").and_then(Value::as_str),
            Some("completed")
        );
        assert!(metadata.get("host_result").is_none());
        assert!(
            metadata
                .get("payload")
                .and_then(|payload| payload.get("conversation_state"))
                .is_none()
        );
    }

    /// Verifies that dedicated `task` tool cards use the robot dispatch title and upgrade from running to completed when the result arrives.
    /// 验证专属 `task` 工具卡片会使用机器人调度标题，并在结果到达后从进行中升级为已完成。
    #[test]
    fn merge_tool_display_messages_formats_task_session_cards() {
        let messages = vec![
            SessionMessageEntry {
                id: String::from("task-call-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 task 工具"),
                body: String::from("## 调用参数\n\n```json\n{\"description\":\"审查仓库\"}\n```"),
                metadata: Some(json!({
                    "tool_call_id": "call-task-1",
                    "tool_name": "task",
                    "arguments": {
                        "description": "审查仓库",
                        "subagent": "explorer",
                    },
                })),
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("task-result-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("调用 task 工具"),
                body: String::from("## 工具返回\n\n子代理已完成。"),
                metadata: Some(json!({
                    "tool_call_id": "call-task-1",
                    "tool_name": "task",
                    "kind": "task_session",
                    "payload": {
                        "task_id": "00000000-0000-0000-0000-000000000001",
                        "description": "审查仓库",
                        "subagent": "explorer",
                        "execution_status": "completed",
                    },
                })),
                created_at: String::from("2"),
            },
        ];

        let merged = merge_tool_display_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "调度 审查仓库（explorer） ✅ 完成");
        assert_eq!(
            merged[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    /// Verifies generated attachments can be recovered from flattened runtime-event metadata so inline media cards and tool modals work immediately after one successful capability call.
    /// 验证可以从扁平化运行时事件元数据中恢复生成附件，从而让内联媒体卡片与工具弹层在能力调用成功后立即生效。
    #[test]
    fn generated_attachments_are_parsed_from_flattened_runtime_metadata() {
        let attachments = generated_attachments_from_tool_metadata(&json!({
            "tool_call_id": "call-media-1",
            "tool_result_id": "tool-result-call-media-1",
            "kind": "generated_attachments",
            "payload": {
                "outputs": [{
                    "kind": "audio",
                    "file_name": "demo.mp3",
                    "local_path": "D:/demo.mp3",
                    "preview_local_path": "D:/demo.preview.png",
                    "mime_type": "audio/mpeg",
                    "asset_id": "asset-1",
                    "byte_len": 2048,
                }],
            },
        }));

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].kind, ConversationAttachmentKind::Audio);
        assert_eq!(attachments[0].file_name, "demo.mp3");
        assert_eq!(attachments[0].local_path, "D:/demo.mp3");
        assert_eq!(
            attachments[0].preview_local_path.as_deref(),
            Some("D:/demo.preview.png")
        );
    }

    #[test]
    fn merge_tool_display_messages_prefers_terminal_task_payload_over_stale_running_result() {
        let messages = vec![
            SessionMessageEntry {
                id: String::from("task-call-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 task 工具"),
                body: String::from("## 调用参数"),
                metadata: Some(json!({
                    "tool_call_id": "call-task-1",
                    "tool_name": "task",
                    "kind": "task_session",
                    "payload": {
                        "task_id": "count_10",
                        "task_session_id": "00000000-0000-0000-0000-000000000001",
                        "display_title": "数数到10（worker）",
                        "description": "数数到10",
                        "subagent": "worker",
                        "execution_status": "completed",
                    },
                })),
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("task-result-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("调用 task 工具"),
                body: String::from("## 工具返回\n\n运行中。"),
                metadata: Some(json!({
                    "tool_call_id": "call-task-1",
                    "tool_name": "task",
                    "kind": "task_session",
                    "payload": {
                        "task_id": "count_10",
                        "task_session_id": "00000000-0000-0000-0000-000000000001",
                        "display_title": "数数到10（worker）",
                        "description": "数数到10",
                        "subagent": "worker",
                        "execution_status": "running",
                    },
                })),
                created_at: String::from("2"),
            },
        ];

        let merged = merge_tool_display_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "调度 数数到10（worker） ✅ 完成");
        assert_eq!(
            merged[0]
                .metadata
                .as_ref()
                .and_then(task_session_payload_from_tool_metadata)
                .and_then(|payload| payload.get("execution_status"))
                .and_then(Value::as_str),
            Some("completed")
        );
    }
}

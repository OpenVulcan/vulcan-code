//! Main application state and view composition for the desktop scaffold.
//! 桌面端骨架的主应用状态与界面组合逻辑。
mod about_licenses;
mod ai_capability_runtime;
mod ai_capability_state;
mod attachment_management;
mod composer_attachments;
mod composer_command_state;
mod configuration;
mod configuration_state;
mod context_menu;
mod diff_state;
mod helpers;
mod integration_form_state;
mod interaction_state;
mod luaskills_progress_state;
mod media_preview;
mod message;
pub(crate) mod model;
mod oauth_flow;
mod panel_navigation_state;
mod pending_prompt_queue;
mod perf;
mod question_modal;
mod session_live_runtime;
#[path = "../session/loading.rs"]
mod session_loading;
#[path = "../session/projection.rs"]
mod session_projection;
mod session_projection_state;
mod session_prompt_compaction;
mod session_state;
mod session_transcript_runtime;
mod subagent_runtime;
mod task_tree;
#[cfg(test)]
mod tests;
mod tool_projection;
mod transcript_state;
mod turn_stats_state;
mod ui_state;
mod views;
mod workbench_model_state;
mod workspace_state;

use self::about_licenses::*;
use self::ai_capability_state::{
    AiCapabilityRoutingPickerState, AiCapabilityTargetPickerState, AiCapabilityVendorEditorState,
    AiCapabilityVendorPickerState, WorkbenchAiCapabilitySection,
};
use self::attachment_management::{
    ProjectAttachmentCleanupExecutionOutcome, ProjectAttachmentCleanupExecutionRequest,
    ProjectAttachmentCleanupTaskContext,
};
use self::composer_attachments::ComposerUndoSnapshot;
use self::composer_command_state::{
    ComposerSlashCommand, ComposerSlashCommandState, build_composer_slash_command_state,
};
use self::helpers::*;
use self::oauth_flow::*;
use self::perf::*;
use self::session_projection_state::SessionAgentViewProjectionSnapshot;
use self::session_prompt_compaction::{
    PromptCompactionExecution, PromptCompactionJobRequest, execute_prompt_compaction_job,
};
use self::turn_stats_state::TurnStatsMergeAnchor;

use self::configuration_state::{
    AgentChoice, CommandShellPickerChoice, ProjectPromptSettingsDraft, PromptPersonalityChoice,
    ProviderEditorState, ProviderFormState, ProviderModelOverrideEditorState,
    ProviderModelPickerState,
};
use self::context_menu::{WorkbenchContextMenuItem, WorkbenchContextMenuState};
use self::diff_state::{
    DiffPreviewPayload, DiffVisualSliceRange, VisibleDiffHunkSlice, WindowControlIcon,
};
use self::integration_form_state::{
    AgentProfileFormState, DEFAULT_MCP_WORKING_DIRECTORY, McpKeyValueRowState, McpServerChoice,
    McpServerFormState, McpTextRowState,
};
use self::interaction_state::{
    HoveredDiffLine, PendingAiCapabilityRouteDrag, PendingMainTaskPanelResize,
    PendingPendingPromptDrag, PendingTaskProjectInteraction, PendingWindowTitleBarDrag,
    WindowTitleBarPointer,
};
use self::luaskills_progress_state::{
    LuaSkillsOperationProgressState, LuaSkillsOperationVisualKind,
};
use self::message::Message;
use self::panel_navigation_state::{
    AboutLicenseCatalogLoadTrigger, MainDisplayPanel, PromptComposerInspectorLayer,
    SettingsSection, main_display_panel_icon_svg_bytes, prompt_composer_inspector_display_text,
    prompt_composer_inspector_segment_key,
};
use self::session_state::{
    ActiveSessionCompactionState, ActiveSessionRunState, ActiveTranscriptDragAutoScroll,
    SelectedSubagentTaskPanel, SessionPromptCompactionTrigger, SubagentTaskPanelState,
    SubagentTaskRunState,
};
use self::task_tree::TaskTreeActionIcon;
use self::tool_projection::*;
use self::transcript_state::{
    CommandRunToolModalSummary, CommandSessionToolModalSummary, ModalSurfacePayload,
    ThinkingModalState, ToolModalPresentation, ToolModalState,
};
use self::ui_state::{
    ComposerScrollbarLayout, GeneratedAudioPlaybackState, WorkbenchImagePreviewModalState,
    WorkbenchNoticeConfirmAction, WorkbenchNoticeModalState, WorkbenchQuestionAnswerDraft,
    WorkbenchQuestionModalOrigin, WorkbenchQuestionModalStage, WorkbenchQuestionModalState,
};
use self::workbench_model_state::{
    WorkbenchModelChoice, WorkbenchOpenRouterEndpointChoice,
    WorkbenchOpenRouterEndpointPickerState, WorkbenchReasoningEffort,
    fetch_openrouter_endpoint_choices_for_model,
};
use self::workspace_state::{
    AgentTranscriptSurfaceMode, AiCapabilityRouteDropTarget, ArchivedDeleteConfirmationState,
    ArchivedDeleteOutcome, ArchivedDeleteTarget, PendingPromptDropTarget,
    PendingWorkspaceHistorySearchNavigation, PromptResourceWatchKind,
    SessionInlineAgentViewDocumentCache, StartupLifecycle, TaskProjectDropTarget,
    WorkspaceHistorySearchPanelState, WorkspaceHistorySearchScope,
};
use crate::agent_profile::{self, AgentProfileCatalog, SYSTEM_DEFAULT_AGENT_CHOICE_ID};
use crate::agent_runtime::config::{
    EXPLORE_AGENT_ID, PLAN_AGENT_ID, WORKSPACE_COORDINATOR_AGENT_ID,
};
use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::provider_protocol_bridge::{
    attach_tool_name_to_runtime_metadata, build_tool_result_runtime_metadata,
};
use crate::agent_runtime::provider_stream_projection::is_provider_tool_boundary_runtime_event;
use crate::agent_runtime::question_runtime::{
    QuestionOption, QuestionPrompt, QuestionRequest, build_question_reply_markdown,
};
use crate::agent_runtime::task_runtime::DetachedTaskRuntimeEvent;
use crate::agent_runtime::{
    self, ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
    ExecutionStreamEvent, RuntimeFoundation, RuntimeSkillKind, stream_runtime_run,
};
use crate::agent_view::{
    AgentViewBuildOptions, agent_view_block_collapsed_by_default, agent_view_block_id,
    agent_view_document_raw_source, build_agent_view_document_with_options,
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
    ContextBudgetSnapshot, ContextCompactionPolicy, apply_context_budget_anchor_baseline,
    build_projected_session_context_budget_snapshot,
};
use crate::context_compaction::{
    CompactionPlan, CompactionSkillInput, CompactionSkillResult, into_session_derived_record,
    plan_compaction,
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
    SessionCompactionEntry, SessionComposerDraftState, SessionId, SessionMessageEntry,
    SessionMessageKind, SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry,
    SessionState, SessionSummary, SessionTitleSource, SessionTurnStats, SessionTurnStatsStatus,
    TaskProjectSummary,
};
use crate::project_temp_maintenance;
use crate::provider_capabilities::resolve_effective_model_capabilities;
use crate::provider_config::{
    self, AiToolKind, ModelChoice, NativeBridgeRoutingMode, PersistedAiToolConfiguration,
    PersistedControllerConfiguration, PersistedMcpTransportKind, PersistedModelOverrideEntry,
    PersistedProviderEntry, ProviderChoice, ProviderConfigurationFile, ProviderFieldKey,
    ProviderRuntimeKind, ReasoningSelectionOption, SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS,
    WebSearchRoutingMode,
};
use crate::selectable_transcript::{
    SelectableTranscriptCommand, SelectableTranscriptContextMenuMediaKind,
    SelectableTranscriptContextMenuRequest, SelectableTranscriptDragAutoScroll,
    SelectableTranscriptDragAutoScrollDirection, SelectableTranscriptInteraction,
    SelectableTranscriptSurfaceId, clear_selectable_transcript_render_cache_stash,
    view_selectable_rendered_transcript_owned_with_context_menu,
    view_selectable_rendered_transcript_shared_with_context_menu,
    view_selectable_rendered_transcript_with_context_menu,
};
use crate::skills::{
    ExploreSkillInput, SessionDerivedRecord, SessionDerivedSkillChain, SummarySkillInput,
    TitleAgentInput,
};
use crate::style;
use crate::theme::{self, ThemePaletteExt};
use crate::tooling::change_set::{
    ChangeSetFileChangeKind, ChangeSetFileSummary, ChangeSetModalSummary, ChangeSetMode,
    change_set_modal_summary_from_payload, compact_change_set_host_result,
};
use crate::tooling::command_session_runtime::{CommandSessionSnapshot, CommandSessionStatus};
use crate::wheel_passthrough::{WheelIntercept, WheelPassthrough};
use crate::workspace_path::normalize_workspace_path_text;
use crate::workspace_store::{
    self, SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS, SESSION_HISTORY_SEARCH_RESULT_LIMIT,
    SessionHistoryPage, SessionHydrationSnapshot, SessionMaintenanceReport,
    WorkspaceHistorySearchPage, WorkspaceSnapshot, WorkspaceSnapshotPersistOutcome,
};
use crate::{AppLocale, I18nArg, I18nKey, LanguageManager};
use iced::advanced::widget::operation::focusable::unfocus;
use iced::widget::operation;
use iced::widget::pane_grid::{self, Configuration, PaneGrid};
use iced::widget::{
    PickList, button, checkbox, container, image as iced_image, mouse_area,
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
use luaskills::{RuntimeSkillOperationProgressEvent, SkillLifecycleAction};
use rodio::{Decoder, OutputStream, Sink};
use serde::Deserialize;
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
use vulcan_markdown as markdown;

/// Bundled SVG mark bytes used in the header.
/// 头部区域使用的内置 SVG 标识字节。
const LOGO_ASSET_BYTES: &[u8] = include_bytes!("../../resources/vulcan-mark.svg");

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

/// Seed Markdown reply rendered in the example ask-debug preview before any live answers are written back.
/// 在写回真实回答之前展示在示例 ask 调试预览中的初始 Markdown 回复。
const SAMPLE_ASK_DEBUG_PREVIEW_MARKDOWN: &str = r#"# Ask Debug Preview

The final Markdown reply for the `ask` tool will appear here.

- Supports selection and copy testing.
- Does not allow direct editing inside the preview surface.
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

/// Builds one read-only text-editor buffer used by one About-page license document body.
/// 构建供单份关于页许可证文档正文使用的一份只读文本编辑器缓冲区。
fn build_about_license_document_buffer(body: &str) -> text_editor::Content {
    text_editor::Content::with_text(body)
}

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

/// Shared animation tick used by lightweight runtime progress labels such as "requesting" and "thinking".
/// 供“正在请求”与“正在思考”等轻量运行进度文案使用的共享动画 Tick。
const SESSION_RUNTIME_PROGRESS_TICK_INTERVAL: Duration = Duration::from_millis(450);

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
/// Stable widget identifier used by the visible ask/question custom-answer input inside the blocking modal.
/// 阻塞式 ask/question 弹窗内部可见自定义回答输入框使用的稳定部件标识。
const WORKBENCH_QUESTION_MODAL_CUSTOM_INPUT_ID: &str =
    "vulcan-code.workbench-question-modal-custom-input";

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

/// Shared maximum visible height used by the inline slash-command suggestion panel shown under the composer.
/// 显示在提示词输入框下方的内联 slash 指令建议面板使用的共享最大可视高度。
const COMPOSER_SLASH_COMMAND_PANEL_MAX_HEIGHT: f32 = 156.0;

/// Shared fixed row height used by one slash-command suggestion item.
/// 单条 slash 指令建议项使用的共享固定行高。
const COMPOSER_SLASH_COMMAND_ROW_HEIGHT: f32 = 46.0;

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

/// Shared fixed width used by the inline OpenRouter secondary-provider trigger surface.
/// 工作台内联 OpenRouter 二级供应商触发表面使用的共享固定宽度。
const WORKBENCH_OPENROUTER_ENDPOINT_SELECTOR_WIDTH: f32 = 170.0;

/// Shared fixed width used by the compact workbench web-search selector placed beside the model trigger.
/// 放置在模型触发器旁侧的紧凑工作台网页搜索选择器使用的共享固定宽度。
const WORKBENCH_AI_CAPABILITY_QUICK_CONTROL_WIDTH: f32 = 150.0;

/// Shared fixed width used by the compact reasoning-effort selector placed before the send button.
/// 放置在发送按钮之前的紧凑推理级别选择器使用的共享固定宽度。
const WORKBENCH_REASONING_SELECTOR_WIDTH: f32 = 112.0;

/// Shared fixed width used by the in-app workbench model-picker modal card.
/// 工作台内嵌模型选择弹窗卡片使用的共享固定宽度。
const WORKBENCH_MODEL_PICKER_MODAL_WIDTH: f32 = 680.0;

/// Shared fixed width used by the in-app OpenRouter secondary-provider picker modal card.
/// 工作台内嵌 OpenRouter 二级供应商选择弹窗卡片使用的共享固定宽度。
const WORKBENCH_OPENROUTER_ENDPOINT_PICKER_MODAL_WIDTH: f32 = 760.0;

/// Shared fixed height used by the scrollable result list inside the OpenRouter secondary-provider picker modal.
/// OpenRouter 二级供应商选择弹窗内部滚动结果列表使用的共享固定高度。
const WORKBENCH_OPENROUTER_ENDPOINT_PICKER_LIST_HEIGHT: f32 = 360.0;

/// Poll interval used to mirror pending ask/question runtime requests into the main workbench modal layer.
/// 用于把 ask/question 运行时待回答请求镜像到主工作台弹层的轮询时间间隔。
const QUESTION_RUNTIME_POLL_TICK_INTERVAL: Duration = Duration::from_millis(250);

/// Poll interval used to mirror queued LuaSkills lifecycle progress events into the management panel.
/// 用于把已排队 LuaSkills 生命周期进度事件镜像到管理面板的轮询时间间隔。
const LUASKILLS_PROGRESS_POLL_TICK_INTERVAL: Duration = Duration::from_millis(120);

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

/// Shared fixed width used by the About-page license detail modal card.
/// 关于页面许可证详情弹窗卡片使用的共享固定宽度。
const WORKBENCH_ABOUT_LICENSE_MODAL_WIDTH: f32 = 920.0;

/// Shared fixed height used by the About-page license detail modal card.
/// 关于页面许可证详情弹窗卡片使用的共享固定高度。
const WORKBENCH_ABOUT_LICENSE_MODAL_HEIGHT: f32 = 700.0;

/// Shared fixed height used by the About-page license detail modal header.
/// 关于页面许可证详情弹窗标题栏使用的共享固定高度。
const WORKBENCH_ABOUT_LICENSE_MODAL_HEADER_HEIGHT: f32 = 40.0;

/// Shared fixed width used by the blocking ask/question modal card.
/// 阻塞式 ask/question 弹窗卡片使用的共享固定宽度。
const WORKBENCH_QUESTION_MODAL_WIDTH: f32 = 920.0;

/// Shared fixed height used by the blocking ask/question modal card.
/// 阻塞式 ask/question 弹窗卡片使用的共享固定高度。
const WORKBENCH_QUESTION_MODAL_HEIGHT: f32 = 680.0;

/// Shared fixed height used by the blocking ask/question modal header.
/// 阻塞式 ask/question 弹窗标题栏使用的共享固定高度。
const WORKBENCH_QUESTION_MODAL_HEADER_HEIGHT: f32 = 40.0;

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

/// Builds one best-effort turn-statistics snapshot from the transcript rows bounded by the provided terminal closing record.
/// 根据给定终态闭合记录所界定的转录行，构建一份最佳努力的轮次统计快照。
fn build_session_turn_stats_from_state(
    conversation_state: &crate::conversation_protocol::SessionConversationState,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
) -> Option<SessionTurnStats> {
    let records = &conversation_state.records;
    let closing_index = records
        .iter()
        .position(|record| record.id == closing_record_id)?;
    let boundary_index = records[..closing_index]
        .iter()
        .rposition(|record| {
            matches!(
                record.record_kind,
                SessionMessageRecordKind::SummaryOutput
                    | SessionMessageRecordKind::TurnTerminated
                    | SessionMessageRecordKind::TurnStats
            )
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let start_index = records[boundary_index..=closing_index]
        .iter()
        .position(|record| {
            matches!(record.kind, SessionMessageKind::User)
                && record.visible
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .map(|offset| boundary_index + offset)
        .unwrap_or(boundary_index);
    let turn_records = &records[start_index..=closing_index];
    if turn_records.is_empty() {
        return None;
    }
    let visible_user_record_count_before_turn = records[..start_index]
        .iter()
        .filter(|record| {
            matches!(record.kind, SessionMessageKind::User)
                && record.visible
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .count();
    let turn_start_message_index = canonical_turn_start_message_index(
        conversation_state,
        visible_user_record_count_before_turn,
    )
    .unwrap_or(0);
    let turn_messages = &conversation_state.messages[turn_start_message_index..];

    let mut stats = SessionTurnStats {
        status,
        provider_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
        duration_ms: None,
        output_tokens_per_second: None,
    };

    stats.tool_call_count = count_turn_tool_calls_from_records(turn_records);

    for message in turn_messages {
        match &message.payload {
            crate::conversation_protocol::SessionConversationPayload::Assistant {
                usage,
                tool_calls,
                ..
            } => {
                if assistant_payload_contributes_to_turn_stats(message) {
                    stats.provider_call_count = stats.provider_call_count.saturating_add(1);
                }
                if stats.tool_call_count == 0 {
                    stats.tool_call_count = stats.tool_call_count.saturating_add(tool_calls.len());
                }
                let Some(usage) = usage.as_ref() else {
                    continue;
                };
                stats.input_tokens = stats
                    .input_tokens
                    .saturating_add(usage.input_tokens.unwrap_or(0));
                stats.output_tokens = stats
                    .output_tokens
                    .saturating_add(usage.output_tokens.unwrap_or(0));
                stats.reasoning_tokens = stats
                    .reasoning_tokens
                    .saturating_add(usage.reasoning_tokens.unwrap_or(0));
                stats.total_tokens =
                    stats
                        .total_tokens
                        .saturating_add(usage.total_tokens.unwrap_or_else(|| {
                            usage
                                .input_tokens
                                .unwrap_or(0)
                                .saturating_add(usage.output_tokens.unwrap_or(0))
                                .saturating_add(usage.reasoning_tokens.unwrap_or(0))
                        }));
                if let Some(cached_input_tokens) = usage.cached_input_tokens {
                    let accumulated = stats.cached_input_tokens.unwrap_or(0);
                    stats.cached_input_tokens =
                        Some(accumulated.saturating_add(cached_input_tokens));
                }
                if let Some(cache_creation_input_tokens) = usage.cache_creation_input_tokens {
                    let accumulated = stats.cache_creation_input_tokens.unwrap_or(0);
                    stats.cache_creation_input_tokens =
                        Some(accumulated.saturating_add(cache_creation_input_tokens));
                }
            }
            crate::conversation_protocol::SessionConversationPayload::ToolResult { .. }
            | crate::conversation_protocol::SessionConversationPayload::UserText { .. } => {}
        }
    }

    if let Some(provider_request_count_override) = provider_request_count_override
        && provider_request_count_override > 0
    {
        stats.provider_call_count = provider_request_count_override;
    }

    let started_at = turn_records
        .iter()
        .find(|record| {
            record.id != closing_record_id
                && record.record_kind != SessionMessageRecordKind::SummaryOutput
                && record.record_kind != SessionMessageRecordKind::TurnTerminated
                && record.record_kind != SessionMessageRecordKind::TurnStats
                && !record.created_at.trim().is_empty()
        })
        .and_then(|record| parse_unix_millis_text(&record.created_at))
        .or_else(|| {
            turn_records
                .iter()
                .find(|record| !record.created_at.trim().is_empty())
                .and_then(|record| parse_unix_millis_text(&record.created_at))
        });
    let finished_at = turn_records
        .iter()
        .rev()
        .find(|record| record.id == closing_record_id)
        .and_then(|record| parse_unix_millis_text(&record.created_at));
    stats.duration_ms = match (started_at, finished_at) {
        (Some(started_at), Some(finished_at)) if finished_at >= started_at => {
            Some((finished_at - started_at) as u64)
        }
        _ => None,
    };
    stats.output_tokens_per_second = stats.duration_ms.and_then(|duration_ms| {
        if duration_ms == 0 || stats.output_tokens == 0 {
            return None;
        }
        Some(stats.output_tokens as f64 / (duration_ms as f64 / 1000.0))
    });

    Some(stats)
}

/// Resolves the canonical message index where the active turn begins by matching the count of visible user transcript rows that appeared before the turn boundary.
/// 通过匹配轮次边界之前已出现的可见用户转录条目数量，解析当前活动轮次在规范消息数组中的起始下标。
fn canonical_turn_start_message_index(
    conversation_state: &crate::conversation_protocol::SessionConversationState,
    visible_user_record_count_before_turn: usize,
) -> Option<usize> {
    let user_message_indexes = canonical_user_message_indexes_for_turn_mapping(conversation_state);
    if user_message_indexes.is_empty() {
        return None;
    }

    user_message_indexes
        .get(visible_user_record_count_before_turn)
        .copied()
        .or_else(|| user_message_indexes.last().copied())
}

/// Returns the canonical user-message indexes used to align visible transcript turns back onto the canonical conversation window.
/// 返回用于将可见转录轮次重新对齐到规范会话窗口上的规范用户消息下标集合。
fn canonical_user_message_indexes_for_turn_mapping(
    conversation_state: &crate::conversation_protocol::SessionConversationState,
) -> Vec<usize> {
    let user_message_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.payload,
                crate::conversation_protocol::SessionConversationPayload::UserText { .. }
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let protocol_user_indexes = conversation_state
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message.payload,
                crate::conversation_protocol::SessionConversationPayload::UserText { .. }
            )
            .then_some((&message.id, index))
        })
        .filter(|(message_id, _)| message_id.contains("-protocol-user-"))
        .map(|(_, index)| index)
        .collect::<Vec<_>>();

    if protocol_user_indexes.is_empty() {
        user_message_indexes
    } else {
        protocol_user_indexes
    }
}

/// Counts unique tool-call records inside the current turn using transcript metadata so UI projection ids never affect the final statistic.
/// 使用转录元数据统计当前轮次中的唯一工具调用记录数量，确保界面投影标识不会影响最终统计值。
fn count_turn_tool_calls_from_records(
    turn_records: &[crate::conversation_protocol::SessionConversationRecord],
) -> usize {
    let mut unique_tool_call_ids = std::collections::BTreeSet::new();
    for record in turn_records {
        if record.record_kind != SessionMessageRecordKind::ToolCallOutput {
            continue;
        }
        let stable_key = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| record.id.clone());
        unique_tool_call_ids.insert(stable_key);
    }
    unique_tool_call_ids.len()
}

/// Returns whether one assistant payload carries meaningful provider work that should count as a provider call in the turn summary.
/// 返回某条助手载荷是否承载了应计入轮次摘要的有效 Provider 工作。
fn assistant_payload_contributes_to_turn_stats(
    message: &crate::conversation_protocol::SessionConversationMessage,
) -> bool {
    match &message.payload {
        crate::conversation_protocol::SessionConversationPayload::Assistant {
            text,
            reasoning,
            usage,
            tool_calls,
            ..
        } => {
            !text.trim().is_empty()
                || !reasoning.trim().is_empty()
                || !tool_calls.is_empty()
                || usage.is_some()
        }
        _ => false,
    }
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
    let projected_index_by_id = projected_messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.id.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut consumed_projected_ids = BTreeSet::new();
    let mut merged_messages =
        Vec::with_capacity(existing_messages.len().max(projected_messages.len()));
    let mut latest_existing_projection_index = existing_messages
        .iter()
        .filter_map(|message| projected_index_by_id.get(&message.id).copied())
        .max();

    for (existing_index, existing_message) in existing_messages.iter().enumerate() {
        if let Some(projected_message) = projected_by_id.get(&existing_message.id).cloned() {
            consumed_projected_ids.insert(existing_message.id.clone());
            merged_messages.push(projected_message);
        } else if let Some(projected_message) = projected_messages
            .iter()
            .enumerate()
            .find(|(projected_index, projected_message)| {
                !consumed_projected_ids.contains(&projected_message.id)
                    && turn_stats_message_semantically_matches(
                        existing_messages,
                        existing_index,
                        existing_message,
                        &projected_messages,
                        *projected_index,
                        projected_message,
                    )
            })
            .map(|(_, projected_message)| projected_message.clone())
        {
            if let Some(projected_index) = projected_index_by_id.get(&projected_message.id).copied()
            {
                latest_existing_projection_index = Some(
                    latest_existing_projection_index
                        .map(|latest_index| latest_index.max(projected_index))
                        .unwrap_or(projected_index),
                );
            }
            consumed_projected_ids.insert(projected_message.id.clone());
            merged_messages.push(projected_message);
        } else {
            merged_messages.push(existing_message.clone());
        }
    }

    for (projected_index, projected_message) in projected_messages.into_iter().enumerate() {
        if consumed_projected_ids.contains(&projected_message.id) {
            continue;
        }
        if latest_existing_projection_index
            .is_some_and(|latest_index| projected_index <= latest_index)
        {
            continue;
        }
        merged_messages.push(projected_message);
    }

    merged_messages
}

/// Returns whether two visible turn-statistics rows should be treated as the same logical row even when their runtime ids or recalculated payload fields drift after reload or reprojection.
/// 返回两条可见轮次统计行在重载或重新投影后即使运行时 id 或重算后的载荷字段漂移，也应视为同一逻辑行。
fn turn_stats_message_semantically_matches(
    existing_messages: &[SessionMessageEntry],
    existing_index: usize,
    existing_message: &SessionMessageEntry,
    projected_messages: &[SessionMessageEntry],
    projected_index: usize,
    projected_message: &SessionMessageEntry,
) -> bool {
    if existing_message.record_kind != SessionMessageRecordKind::TurnStats
        || projected_message.record_kind != SessionMessageRecordKind::TurnStats
        || existing_message.kind != projected_message.kind
        || existing_message.visible != projected_message.visible
        || existing_message.title != projected_message.title
    {
        return false;
    }

    if existing_message.body == projected_message.body
        && existing_message.created_at == projected_message.created_at
        && existing_message.metadata == projected_message.metadata
    {
        return true;
    }

    let existing_closing_record_id =
        turn_stats_message_metadata_string(existing_message, "turn_closing_record_id");
    let projected_closing_record_id =
        turn_stats_message_metadata_string(projected_message, "turn_closing_record_id");
    if existing_closing_record_id.is_some()
        && existing_closing_record_id == projected_closing_record_id
    {
        return true;
    }

    let existing_anchor =
        turn_stats_previous_visible_user_anchor(existing_messages, existing_index);
    let projected_anchor =
        turn_stats_previous_visible_user_anchor(projected_messages, projected_index);
    existing_anchor.is_some() && existing_anchor == projected_anchor
}

/// Returns one structured anchor for a turn-stats row based on the visible user row that immediately precedes it.
/// 根据紧邻其前的可见用户行返回该轮次统计行的结构化锚点。
fn turn_stats_previous_visible_user_anchor(
    messages: &[SessionMessageEntry],
    turn_stats_index: usize,
) -> Option<TurnStatsMergeAnchor> {
    if turn_stats_index == 0 {
        return None;
    }

    let user_message = messages[..turn_stats_index].iter().rev().find(|message| {
        message.visible
            && message.kind == SessionMessageKind::User
            && message.record_kind == SessionMessageRecordKind::Transcript
    })?;
    Some(TurnStatsMergeAnchor {
        user_record_id: Some(user_message.id.clone()),
        user_created_at: Some(user_message.created_at.clone()),
        user_text: Some(turn_stats_anchor_text(user_message)),
    })
}

/// Returns one stable text anchor for a user row by preferring canonical metadata and falling back to the visible body.
/// 为用户行返回稳定文本锚点，优先使用规范元数据，必要时回退到可见正文。
fn turn_stats_anchor_text(message: &SessionMessageEntry) -> String {
    let text = turn_stats_message_metadata_string(message, "canonical_user_text")
        .unwrap_or_else(|| message.body.clone());
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads one string field from turn-stats metadata when it exists and is non-empty.
/// 当轮次统计元数据中存在且非空时读取其中一个字符串字段。
fn turn_stats_message_metadata_string(message: &SessionMessageEntry, key: &str) -> Option<String> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Returns whether one clipboard text payload should enable the composer paste action.
/// 返回某段剪贴板文本载荷是否应启用提示词粘贴动作。
fn composer_clipboard_text_is_pastable(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.is_empty())
}

/// Sanitizes pasted workspace-history search text into one bounded single-line query.
/// 将粘贴的工作台历史搜索文本清洗为一条有界单行查询。
fn sanitize_workspace_history_search_query_input(query: &str) -> String {
    let mut sanitized =
        String::with_capacity(query.len().min(SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS));
    let mut retained_chars = 0usize;
    let mut pending_space = false;

    for ch in query.chars() {
        if retained_chars >= SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS {
            break;
        }

        if ch.is_whitespace() || ch.is_control() {
            pending_space = !sanitized.is_empty();
            continue;
        }

        if pending_space {
            sanitized.push(' ');
            retained_chars += 1;
            pending_space = false;
            if retained_chars >= SESSION_HISTORY_SEARCH_QUERY_MAX_CHARS {
                break;
            }
        }

        sanitized.push(ch);
        retained_chars += 1;
    }

    sanitized
}

/// Returns whether one session-hydration error is caused by missing canonical event-store files on disk.
/// 返回某条会话水合错误是否由磁盘上缺失规范事件存储文件导致。
fn session_event_store_missing_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    let mentions_session_store = normalized.contains(".vulcan\\code\\sessions")
        || normalized.contains(".vulcan/code/sessions")
        || normalized.contains("manifest.json")
        || normalized.contains("events/000001.jsonl")
        || normalized.contains("events\\000001.jsonl");
    let is_not_found = normalized.contains("os error 3")
        || normalized.contains("os error 2")
        || normalized.contains("系统找不到指定的路径")
        || normalized.contains("the system cannot find the path specified")
        || normalized.contains("no such file or directory")
        || normalized.contains("找不到指定的路径");
    mentions_session_store && is_not_found
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
    /// Runtime-only slash-command suggestion state currently derived from the composer text.
    /// 当前根据提示词文本推导出的仅运行时存在的 slash 指令建议状态。
    composer_slash_command: Option<ComposerSlashCommandState>,
    /// Structured multimodal attachments currently staged beside the prompt composer text.
    /// 当前与提示词文本一起暂存的结构化多模态附件集合。
    composer_attachments: Vec<ConversationAttachment>,
    /// Reusable blocking notice modal currently shown above all main-workbench content.
    /// 当前显示在所有主工作台内容之上的可复用阻塞提示弹窗。
    workbench_notice_modal: Option<WorkbenchNoticeModalState>,
    /// Blocking ask/question modal currently shown while one model-visible structured-question tool call is waiting for the user.
    /// 当前在某条模型可见结构化提问工具调用等待用户时显示的阻塞式 ask/question 弹窗。
    active_question_modal: Option<WorkbenchQuestionModalState>,
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
    /// Read-only Markdown preview buffer dedicated to the example ask-debug sandbox.
    /// 专供示例 ask 调试沙盒使用的只读 Markdown 预览缓冲区。
    example_preview_markdown_buffer: text_editor::Content,
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
    /// Latest prompt-composer inspector Markdown cached for copy and selection.
    /// 为复制与选择缓存的最近提示词组合检查器 Markdown。
    prompt_composer_inspector_markdown: String,
    /// Latest prompt-composer system-layer text cached for focused copy.
    /// 为定向复制缓存的最近提示词组合系统层文本。
    prompt_composer_inspector_system_text: String,
    /// Latest prompt-composer developer-layer text cached for focused copy.
    /// 为定向复制缓存的最近提示词组合开发者层文本。
    prompt_composer_inspector_developer_text: String,
    /// Latest prompt-composer tool-metadata-layer text cached for focused copy.
    /// 为定向复制缓存的最近提示词组合工具元数据层文本。
    prompt_composer_inspector_tool_metadata_text: String,
    /// Latest prompt-composer user-context-layer text cached for focused copy.
    /// 为定向复制缓存的最近提示词组合用户上下文层文本。
    prompt_composer_inspector_user_text: String,
    /// Read-only Markdown buffer rendered in the prompt-composer inspector card.
    /// 渲染在提示词组合检查器卡片中的只读 Markdown 缓冲区。
    prompt_composer_inspector_buffer: text_editor::Content,
    /// Read-only text buffers rendered by individual prompt-composer inspector layers.
    /// 由各个提示词组合检查器分层渲染的只读文本缓冲区。
    prompt_composer_inspector_layer_buffers:
        BTreeMap<PromptComposerInspectorLayer, text_editor::Content>,
    /// Full prompt-composer segment bodies keyed by stable inspector segment keys.
    /// 以稳定检查器节点键索引的完整提示词组合片段正文。
    prompt_composer_inspector_segment_texts: BTreeMap<String, String>,
    /// Read-only text buffers rendered by expanded prompt-composer segment details.
    /// 由已展开提示词组合节点明细渲染的只读文本缓冲区。
    prompt_composer_inspector_segment_buffers: BTreeMap<String, text_editor::Content>,
    /// Expanded prompt-composer inspector layers.
    /// 当前展开的提示词组合检查器分层集合。
    prompt_composer_inspector_expanded_layers: BTreeSet<PromptComposerInspectorLayer>,
    /// Expanded prompt-composer inspector segment identifiers.
    /// 当前展开的提示词组合检查器节点标识集合。
    prompt_composer_inspector_expanded_segments: BTreeSet<String>,
    /// Application-side license catalog rendered by the dedicated About settings page.
    /// 由专用关于设置页渲染的应用侧许可证目录。
    about_license_catalog: AboutLicenseCatalog,
    /// Whether the About-page license catalog is currently loading on a background worker thread.
    /// 关于页许可证目录当前是否正在后台工作线程中加载。
    about_license_catalog_loading: bool,
    /// Latest load failure captured for the About-page license catalog, if any.
    /// 关于页许可证目录最近一次捕获到的加载失败信息（如果存在）。
    about_license_catalog_load_error: Option<String>,
    /// Monotonic revision used to ignore stale About-page load completions that finish out of order.
    /// 用于忽略乱序完成的过期关于页加载结果的单调修订号。
    about_license_catalog_load_revision: u64,
    /// Stable identifier of the single About-page document whose full body is currently expanded.
    /// 当前在关于页中展开完整正文的单份文档稳定标识。
    about_license_active_document_id: Option<String>,
    /// Whether the currently expanded About-page document buffer is still being prepared on a background worker.
    /// 当前展开的关于页文档缓冲区是否仍在后台工作线程中准备。
    about_license_document_buffer_loading: bool,
    /// Latest load failure captured for the currently expanded About-page document buffer, if any.
    /// 当前展开的关于页文档缓冲区最近一次捕获到的加载失败信息（如果存在）。
    about_license_document_buffer_load_error: Option<String>,
    /// Monotonic revision used to ignore stale About-page document-buffer completions that finish out of order.
    /// 用于忽略乱序完成的过期关于页文档缓冲区结果的单调修订号。
    about_license_document_buffer_load_revision: u64,
    /// Read-only text-editor buffers backing the About-page license document surfaces.
    /// 为关于页许可证文档表面提供支撑的只读文本编辑器缓冲区集合。
    about_license_document_buffers: BTreeMap<String, text_editor::Content>,
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
    /// Latest per-session API-request preview markdown cached before optional editor-buffer materialization.
    /// 在可选编辑器缓冲区物化前缓存的按会话归属最近 API 请求预览 Markdown 原文。
    session_last_api_request_markdowns: BTreeMap<SessionId, String>,
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
    /// Per-session active prompt-compaction task state keyed by session identifier.
    /// 以会话标识为键保存的按会话划分活动提示词压缩任务状态。
    active_session_compactions: BTreeMap<SessionId, ActiveSessionCompactionState>,
    /// Session identifiers currently waiting for the next provider-visible response chunk after a request has already been sent.
    /// 已经发送请求、但仍在等待下一段 Provider 可见响应块的会话标识集合。
    pending_provider_response_sessions: BTreeSet<SessionId>,
    /// Shared animation frame used by transient runtime progress labels.
    /// 供瞬时运行进度文案复用的共享动画帧编号。
    runtime_progress_animation_frame: usize,
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
    /// Latest fine-grained LuaSkills lifecycle progress snapshot mirrored from the runtime callback queue.
    /// 从运行时回调队列镜像得到的最新细粒度 LuaSkills 生命周期进度快照。
    luaskills_operation_progress: Option<LuaSkillsOperationProgressState>,
    /// Persisted provider configuration file loaded from `~/.vulcan/vulcan-code.json`.
    /// 从 `~/.vulcan/vulcan-code.json` 载入的持久化供应商配置文件。
    provider_configuration: ProviderConfigurationFile,
    /// Editable project-level prompt settings draft for the currently selected workspace.
    /// 当前选中工作区的可编辑项目级提示词设置草稿。
    project_prompt_settings_draft: ProjectPromptSettingsDraft,
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
    /// Whether one project-scoped attachment cleanup preview is currently running in the background.
    /// 当前是否有一条项目级附件清理预览正在后台运行。
    project_attachment_cleanup_preview_in_flight: bool,
    /// Whether one project-scoped attachment cleanup execution is currently running in the background.
    /// 当前是否有一条项目级附件清理执行正在后台运行。
    project_attachment_cleanup_execution_in_flight: bool,
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
    /// In-memory OpenRouter endpoint-picker state shown only when the active model belongs to OpenRouter.
    /// 仅在当前激活模型属于 OpenRouter 时显示的内存态二级供应商选择弹窗状态。
    workbench_openrouter_endpoint_picker: WorkbenchOpenRouterEndpointPickerState,
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
    /// Stateful GUI language manager used to resolve localized text and switch locales at runtime.
    /// 用于解析本地化文本并在运行时切换语言的有状态图形界面语言管理器。
    language_manager: LanguageManager,
    /// Startup transcript turn-page size selected by the user and shared by initial hydration plus manual history paging.
    /// 由用户选择并被启动水合与手动历史分页共享的启动转录轮次分页大小。
    session_transcript_turn_page_size: usize,
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
    /// Latest visible context-budget snapshot mirrored into the workbench surface.
    /// 镜像到工作台表面的最近一次可见上下文预算快照。
    visible_context_budget_snapshot: Option<ContextBudgetSnapshot>,
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

impl VulcanWorkbench {
    /// Returns the currently active GUI locale selected by the user.
    /// 返回用户当前选中的图形界面语言。
    fn current_locale(&self) -> AppLocale {
        self.language_manager.current_locale()
    }

    /// Resolves one localized plain text using the current GUI locale.
    /// 使用当前图形界面语言解析一条普通本地化文本。
    fn t(&self, key: I18nKey) -> String {
        self.language_manager.text(key)
    }

    /// Resolves one localized formatted text using the current GUI locale and named arguments.
    /// 使用当前图形界面语言与命名参数解析一条带格式的本地化文本。
    fn tf(&self, key: I18nKey, args: &[I18nArg<'_>]) -> String {
        self.language_manager.format(key, args)
    }

    /// Returns the localized quick-guide Markdown source rendered in the runtime inspector card.
    /// 返回渲染在运行检查卡片中的本地化 Markdown 快速指引源码。
    fn localized_quick_guide_markdown(&self) -> String {
        self.t(I18nKey::GuideQuickMarkdown)
    }

    /// Rebuilds the quick-guide selectable Markdown buffer after locale or theme changes.
    /// 在语言或主题发生变化后重建快速指引的可选择 Markdown 缓冲区。
    fn rebuild_quick_guide_markdown_buffer(&mut self) {
        let markdown_source = self.localized_quick_guide_markdown();
        let parsed = markdown::Content::parse(&markdown_source);
        self.quick_guide_markdown_buffer = text_editor::Content::with_text(
            &build_selectable_markdown_text(&markdown_source, &parsed, &self.app_theme),
        );
    }

    /// Refreshes the prompt-composer inspector cache from the latest runtime report.
    /// 从最近的运行时报告刷新提示词组合检查器缓存。
    fn refresh_prompt_composer_inspector_buffer(&mut self) {
        let Some(report) =
            crate::agent_runtime::prompt_composer_runtime::latest_runtime_prompt_composer_report()
        else {
            self.prompt_composer_inspector_markdown.clear();
            self.prompt_composer_inspector_system_text.clear();
            self.prompt_composer_inspector_developer_text.clear();
            self.prompt_composer_inspector_tool_metadata_text.clear();
            self.prompt_composer_inspector_user_text.clear();
            self.prompt_composer_inspector_buffer = text_editor::Content::with_text("");
            self.prompt_composer_inspector_layer_buffers.clear();
            self.prompt_composer_inspector_segment_texts.clear();
            self.prompt_composer_inspector_segment_buffers.clear();
            self.prompt_composer_inspector_expanded_segments.clear();
            return;
        };

        if self.prompt_composer_inspector_markdown == report.markdown
            && self.prompt_composer_inspector_system_text == report.system_text
            && self.prompt_composer_inspector_developer_text == report.developer_text
            && self.prompt_composer_inspector_tool_metadata_text == report.tool_metadata_text
            && self.prompt_composer_inspector_user_text == report.user_text
        {
            return;
        }

        self.prompt_composer_inspector_markdown = report.markdown;
        self.prompt_composer_inspector_system_text = report.system_text;
        self.prompt_composer_inspector_developer_text = report.developer_text;
        self.prompt_composer_inspector_tool_metadata_text = report.tool_metadata_text;
        self.prompt_composer_inspector_user_text = report.user_text;
        self.prompt_composer_inspector_buffer = text_editor::Content::with_text("");
        self.prompt_composer_inspector_layer_buffers.clear();
        self.prompt_composer_inspector_segment_texts.clear();
        self.prompt_composer_inspector_segment_buffers.clear();
        self.prompt_composer_inspector_expanded_segments.clear();
        for (index, segment) in report.segments.iter().enumerate() {
            let segment_key = prompt_composer_inspector_segment_key(
                index,
                &segment.target_label,
                &segment.node_id,
            );
            self.prompt_composer_inspector_segment_buffers.insert(
                segment_key.clone(),
                text_editor::Content::with_text(&prompt_composer_inspector_display_text(
                    &segment.body,
                )),
            );
            self.prompt_composer_inspector_segment_texts
                .insert(segment_key, segment.body.clone());
        }
        let expanded_layers = self
            .prompt_composer_inspector_expanded_layers
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for layer in expanded_layers {
            self.ensure_prompt_composer_inspector_layer_buffer(layer);
        }
    }

    /// Lazily materializes one prompt-composer layer editor buffer only when the layer is actually opened.
    /// 仅在提示词组合检查器分层真正展开时，才惰性物化对应编辑器缓冲。
    fn ensure_prompt_composer_inspector_layer_buffer(
        &mut self,
        layer: PromptComposerInspectorLayer,
    ) {
        if self
            .prompt_composer_inspector_layer_buffers
            .contains_key(&layer)
        {
            return;
        }

        let layer_text = prompt_composer_inspector_display_text(
            self.prompt_composer_inspector_layer_text(layer),
        );
        self.prompt_composer_inspector_layer_buffers
            .insert(layer, text_editor::Content::with_text(&layer_text));
    }

    /// Lazily materializes one prompt-composer segment editor buffer only when the node is actually opened.
    /// 仅在提示词组合检查器节点真正展开时，才惰性物化对应编辑器缓冲。
    fn ensure_prompt_composer_inspector_segment_buffer(&mut self, segment_key: &str) {
        if self
            .prompt_composer_inspector_segment_buffers
            .contains_key(segment_key)
        {
            return;
        }

        let Some(segment_text) = self
            .prompt_composer_inspector_segment_texts
            .get(segment_key)
            .cloned()
        else {
            return;
        };
        let display_text = prompt_composer_inspector_display_text(&segment_text);
        self.prompt_composer_inspector_segment_buffers.insert(
            segment_key.to_string(),
            text_editor::Content::with_text(&display_text),
        );
    }

    /// Returns the cached text for one prompt-composer inspector layer.
    /// 返回单个提示词组合检查器分层的缓存文本。
    fn prompt_composer_inspector_layer_text(&self, layer: PromptComposerInspectorLayer) -> &str {
        match layer {
            PromptComposerInspectorLayer::System => &self.prompt_composer_inspector_system_text,
            PromptComposerInspectorLayer::Developer => {
                &self.prompt_composer_inspector_developer_text
            }
            PromptComposerInspectorLayer::ToolMetadata => {
                &self.prompt_composer_inspector_tool_metadata_text
            }
            PromptComposerInspectorLayer::UserContext => &self.prompt_composer_inspector_user_text,
        }
    }

    /// Copies one prompt-composer inspector layer to the clipboard when available.
    /// 在可用时将单个提示词组合检查器分层复制到剪贴板。
    fn copy_prompt_composer_inspector_layer(
        &mut self,
        layer: PromptComposerInspectorLayer,
    ) -> Task<Message> {
        if self
            .prompt_composer_inspector_layer_text(layer)
            .trim()
            .is_empty()
        {
            self.refresh_prompt_composer_inspector_buffer();
        }
        let layer_text = self.prompt_composer_inspector_layer_text(layer).to_string();
        if layer_text.trim().is_empty() {
            self.status_message = format!("{}暂无可复制内容。", layer.label());
            Task::none()
        } else {
            self.status_message = format!("已复制{}提示词。", layer.label());
            clipboard::write(layer_text).discard()
        }
    }

    /// Copies one prompt-composer segment to the clipboard when available.
    /// 在可用时将单个提示词组合片段复制到剪贴板。
    fn copy_prompt_composer_inspector_segment(&mut self, segment_key: String) -> Task<Message> {
        if let Some(segment_text) = self
            .prompt_composer_inspector_segment_texts
            .get(&segment_key)
            .cloned()
        {
            if segment_text.trim().is_empty() {
                self.status_message = String::from("该节点暂无可复制内容。");
                Task::none()
            } else {
                self.status_message = String::from("已复制提示词节点正文。");
                clipboard::write(segment_text).discard()
            }
        } else {
            self.status_message = String::from("该节点正文缓存已失效，请重新执行一次会话。");
            Task::none()
        }
    }

    /// Returns whether entering the About-page section should trigger the first lazy catalog load automatically.
    /// 返回进入关于页分区时是否应自动触发首次延迟目录加载。
    fn should_auto_start_about_license_catalog_reload(&self, section_changed: bool) -> bool {
        section_changed
            && !self.about_license_catalog_loading
            && (self.about_license_catalog_load_revision == 0
                || self.about_license_catalog_load_error.is_some())
    }

    /// Starts one background worker-thread task that prepares the currently requested About-page document buffer on demand.
    /// 按需启动一条后台工作线程任务，为当前请求的关于页文档准备正文缓冲区。
    fn start_about_license_document_buffer_load(&mut self, document_id: String) -> Task<Message> {
        let Some(document) = self.about_license_catalog.document(&document_id).cloned() else {
            self.about_license_document_buffer_loading = false;
            self.about_license_document_buffer_load_error =
                Some(String::from("当前许可证文档正文不可用，无法展开显示。"));
            return Task::none();
        };

        if self
            .about_license_document_buffers
            .contains_key(&document_id)
        {
            self.about_license_active_document_id = Some(document_id);
            self.about_license_document_buffer_loading = false;
            self.about_license_document_buffer_load_error = None;
            return Task::none();
        }

        self.about_license_active_document_id = Some(document_id.clone());
        self.about_license_document_buffer_loading = true;
        self.about_license_document_buffer_load_error = None;
        self.about_license_document_buffer_load_revision = self
            .about_license_document_buffer_load_revision
            .wrapping_add(1);
        let revision = self.about_license_document_buffer_load_revision;

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    document
                        .load_body_text()
                        .map(|body| build_about_license_document_buffer(&body))
                })
                .await
                .map_err(|error| format!("许可证正文后台缓冲任务加入失败：{error}"))
                .and_then(|result| result)
            },
            move |result| Message::AboutLicenseDocumentBufferLoaded(revision, document_id, result),
        )
    }

    /// Applies one finished About-page document-buffer result when it still matches the latest requested revision and document selection.
    /// 应用一条已完成的关于页文档缓冲结果，并在其仍匹配最新请求修订号与当前文档选择时生效。
    fn finish_about_license_document_buffer_load(
        &mut self,
        revision: u64,
        document_id: String,
        result: Result<text_editor::Content, String>,
    ) -> Task<Message> {
        if revision != self.about_license_document_buffer_load_revision
            || self.about_license_active_document_id.as_deref() != Some(document_id.as_str())
        {
            return Task::none();
        }

        self.about_license_document_buffer_loading = false;
        match result {
            Ok(content) => {
                self.about_license_document_buffers
                    .insert(document_id, content);
                self.about_license_document_buffer_load_error = None;
            }
            Err(error) => {
                self.about_license_document_buffer_load_error = Some(error.clone());
                self.status_message = format!("许可证正文加载失败：{error}");
            }
        }

        Task::none()
    }

    /// Starts one background worker-thread task that reloads the About-page license catalog while keeping already cached page state visible.
    /// 启动一条后台工作线程任务以重新加载关于页许可证目录，并保留已缓存的页面状态继续可见。
    fn start_about_license_catalog_reload(
        &mut self,
        trigger: AboutLicenseCatalogLoadTrigger,
    ) -> Task<Message> {
        self.about_license_catalog_loading = true;
        self.about_license_catalog_load_error = None;
        self.about_license_catalog_load_revision =
            self.about_license_catalog_load_revision.wrapping_add(1);
        let revision = self.about_license_catalog_load_revision;
        self.status_message = match trigger {
            AboutLicenseCatalogLoadTrigger::OpenSection => String::from("正在后台加载许可证目录。"),
        };

        Task::perform(
            async move {
                tokio::task::spawn_blocking(AboutLicenseCatalog::load)
                    .await
                    .map_err(|error| format!("许可证后台加载任务加入失败：{error}"))
                    .and_then(|result| result)
            },
            move |result| Message::AboutLicenseCatalogLoaded(revision, result),
        )
    }

    /// Applies one finished About-page license catalog load result when it still matches the latest requested revision.
    /// 应用一条已完成的关于页许可证目录加载结果，并在其仍匹配最新请求修订号时生效。
    fn finish_about_license_catalog_reload(
        &mut self,
        revision: u64,
        result: Result<AboutLicenseCatalog, String>,
    ) -> Task<Message> {
        if revision != self.about_license_catalog_load_revision {
            return Task::none();
        }

        self.about_license_catalog_loading = false;
        match result {
            Ok(catalog) => {
                let document_count = catalog.documents.len();
                let active_document_id = self
                    .about_license_active_document_id
                    .clone()
                    .filter(|document_id| catalog.document(document_id).is_some());
                self.about_license_catalog = catalog;
                self.status_message =
                    format!("许可证目录已加载，共载入 {document_count} 条列表项。");
                self.about_license_catalog_load_error = None;
                self.about_license_document_buffers.clear();
                self.about_license_document_buffer_loading = false;
                self.about_license_document_buffer_load_error = None;
                self.about_license_active_document_id = active_document_id.clone();
                if let Some(document_id) = active_document_id {
                    return self.start_about_license_document_buffer_load(document_id);
                }
            }
            Err(error) => {
                self.about_license_catalog_load_error = Some(error.clone());
                self.status_message = format!("许可证目录加载失败：{error}");
            }
        }
        Task::none()
    }

    /// Rebuilds every cached session transcript projection so locale-sensitive labels refresh immediately after a GUI language change.
    /// 重建全部已缓存会话转录投影，使依赖语言的标签在界面语言切换后立即刷新。
    fn refresh_all_session_agent_view_localizations(&mut self) {
        let session_ids = self.session_messages.keys().copied().collect::<Vec<_>>();
        for session_id in session_ids {
            self.refresh_session_agent_view_buffers(session_id);
        }
    }

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
        let initial_session_transcript_turn_page_size =
            Self::resolve_initial_session_transcript_turn_page_size(&provider_configuration);
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
        let language_manager =
            LanguageManager::new(Self::resolve_initial_locale(&provider_configuration));
        let quick_guide_source = language_manager.text(I18nKey::GuideQuickMarkdown);
        let quick_guide = markdown::Content::parse(&quick_guide_source);
        let quick_guide_markdown_buffer = text_editor::Content::with_text(
            &build_selectable_markdown_text(&quick_guide_source, &quick_guide, &app_theme),
        );
        let about_license_catalog = AboutLicenseCatalog::empty();
        let about_license_document_buffers = BTreeMap::new();

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
            workspace_path: String::from(r"D:\openvulcan\vulcan-code"),
            composer: String::new(),
            composer_buffer: text_editor::Content::with_text(""),
            composer_slash_command: None,
            composer_attachments: Vec::new(),
            workbench_notice_modal: None,
            active_question_modal: None,
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
            example_preview_markdown_buffer: text_editor::Content::with_text(
                SAMPLE_ASK_DEBUG_PREVIEW_MARKDOWN,
            ),
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
            prompt_composer_inspector_markdown: String::new(),
            prompt_composer_inspector_system_text: String::new(),
            prompt_composer_inspector_developer_text: String::new(),
            prompt_composer_inspector_tool_metadata_text: String::new(),
            prompt_composer_inspector_user_text: String::new(),
            prompt_composer_inspector_buffer: text_editor::Content::with_text(""),
            prompt_composer_inspector_layer_buffers: BTreeMap::new(),
            prompt_composer_inspector_segment_texts: BTreeMap::new(),
            prompt_composer_inspector_segment_buffers: BTreeMap::new(),
            prompt_composer_inspector_expanded_layers: BTreeSet::new(),
            prompt_composer_inspector_expanded_segments: BTreeSet::new(),
            about_license_catalog,
            about_license_catalog_loading: false,
            about_license_catalog_load_error: None,
            about_license_catalog_load_revision: 0,
            about_license_active_document_id: None,
            about_license_document_buffer_loading: false,
            about_license_document_buffer_load_error: None,
            about_license_document_buffer_load_revision: 0,
            about_license_document_buffers,
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
            session_last_api_request_markdowns: BTreeMap::new(),
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
            active_session_compactions: BTreeMap::new(),
            pending_provider_response_sessions: BTreeSet::new(),
            runtime_progress_animation_frame: 0,
            next_run_id: 1,
            startup_lifecycle: StartupLifecycle::Bootstrapping {
                message: language_manager.text(I18nKey::StatusInitializingDataPlane),
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
            luaskills_operation_progress: None,
            provider_configuration,
            project_prompt_settings_draft: ProjectPromptSettingsDraft::default(),
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
            project_attachment_cleanup_preview_in_flight: false,
            project_attachment_cleanup_execution_in_flight: false,
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
            workbench_openrouter_endpoint_picker: WorkbenchOpenRouterEndpointPickerState::default(),
            workbench_reasoning_picker_open: false,
            workbench_model_picker_filter: String::new(),
            workbench_model_picker_highlighted_index: None,
            workbench_model_picker_scroll_offset: 0.0,
            workbench_model_picker_viewport_height: WORKBENCH_MODEL_PICKER_LIST_HEIGHT,
            app_theme,
            language_manager: language_manager.clone(),
            session_transcript_turn_page_size: initial_session_transcript_turn_page_size,
            temperature_enabled: false,
            temperature: 0.25,
            context_usage: 0.0,
            context_usage_detail: String::from("尚未生成预算快照"),
            visible_context_budget_snapshot: None,
            generate_plan: false,
            run_cargo_check: true,
            persist_memory: true,
            auto_approve: false,
            status_message: language_manager.text(I18nKey::StatusInitializingDataPlane),
        };

        let _ = application.reconcile_workbench_reasoning_effort_for_active_model();
        application.reload_project_prompt_settings_draft();

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
                    message: self.t(I18nKey::StatusInitializingDataPlane),
                };
                self.status_message = self.t(I18nKey::StatusInitializingDataPlane);
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
                self.luaskills_operation_progress = None;
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
                self.session_last_api_request_markdowns.clear();
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
                self.active_runs.clear();
                self.active_session_compactions.clear();
                self.pending_provider_response_sessions.clear();
                self.runtime_progress_animation_frame = 0;
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
                            message: self.t(I18nKey::StatusInitializingLuaSkills),
                        };
                        self.status_message = self.t(I18nKey::StatusInitializingLuaSkills);

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
                        self.refresh_runtime_foundation();
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
                        self.apply_session_hydration_snapshot(snapshot);
                        if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            let continue_search_navigation =
                                self.maybe_continue_workspace_history_search_navigation(session_id);
                            self.status_message = self.t(I18nKey::StatusSessionHydrated);
                            return Task::batch([
                                self.schedule_session_agent_view_projection_refresh_if_needed(
                                    session_id,
                                ),
                                self.maybe_follow_selected_session_transcript_tail(session_id),
                                self.maybe_follow_selected_session_thinking_overlay_tail(
                                    session_id,
                                ),
                                continue_search_navigation,
                            ]);
                        }

                        return Task::batch([self
                            .schedule_session_agent_view_projection_refresh_if_needed(
                                session_id,
                            )]);
                    }
                    Err(error) => {
                        if session_event_store_missing_error(&error) {
                            let session_title = self
                                .sessions
                                .iter()
                                .chain(self.archived_sessions.iter())
                                .find(|session| session.id == session_id)
                                .map(|session| session.title.clone())
                                .unwrap_or_else(|| format!("会话 {session_id}"));
                            self.show_workbench_confirmation_modal(
                                "会话正文已丢失",
                                format!(
                                    "会话 `{session_title}` 的正文文件已经丢失，当前无法继续加载。\n\n是否按“新会话”状态重建当前会话？重建后会保留原标题，但原有正文与工具记录将不会恢复。"
                                ),
                                "重建会话",
                                "暂不处理",
                                WorkbenchNoticeConfirmAction::RebuildMissingSession {
                                    session_id,
                                },
                            );
                        }
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
                                self.maybe_continue_workspace_history_search_navigation(session_id),
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
                        self.apply_older_session_history_page(page);
                        self.status_message = self.t(I18nKey::StatusOlderHistoryLoaded);
                        return Task::batch([
                            self.maybe_continue_workspace_history_search_navigation(session_id)
                        ]);
                    }
                    Err(error) => {
                        self.status_message = format!("加载更早会话历史失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "加载更早会话历史失败", error);
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
                match result {
                    Ok(outcome) if !outcome.is_clean() => {
                        self.status_message =
                            String::from("工作台状态已保存，但历史搜索索引同步被延后。");
                        for warning in outcome.warnings {
                            self.push_activity(
                                ActivityLevel::Warning,
                                "工作台历史搜索延后同步",
                                warning,
                            );
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.status_message = format!("工作台状态保存失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台状态保存失败", error);
                    }
                }

                next_persist_task
            }
            Message::LuaSkillsCatalogLoaded(result) => {
                self.luaskills_operation_in_flight = false;
                self.luaskills_operation_progress = None;
                if let Some(handle) = self.luaskills_runtime.as_ref() {
                    let _ = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
                }

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
                self.refresh_runtime_foundation();

                Task::none()
            }
            Message::PollLuaSkillsOperationProgressTick => {
                self.poll_luaskills_operation_progress();
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
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Refresh,
                    None,
                    None,
                    String::from("正在刷新 LuaSkills 技能目录。"),
                );
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

                let install_skill_id = if self.luaskills_install_skill_id.trim().is_empty() {
                    None
                } else {
                    Some(self.luaskills_install_skill_id.clone())
                };
                let action = LuaSkillsManagementAction::Install {
                    skill_id: install_skill_id.clone(),
                    source: self.luaskills_install_source.clone(),
                    source_type: self.luaskills_install_source_choice,
                };
                self.luaskills_operation_in_flight = true;
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Install,
                    install_skill_id,
                    Some(String::from("USER")),
                    String::from("正在安装 LuaSkills 技能。"),
                );
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
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Update,
                    Some(skill_id.clone()),
                    None,
                    format!("正在更新技能 `{skill_id}`。"),
                );
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
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Enable,
                    Some(skill_id.clone()),
                    None,
                    format!("正在启用技能 `{skill_id}`。"),
                );
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
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Disable,
                    Some(skill_id.clone()),
                    None,
                    format!("正在停用技能 `{skill_id}`。"),
                );
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
                self.begin_luaskills_panel_operation(
                    LuaSkillsOperationVisualKind::Uninstall,
                    Some(skill_id.clone()),
                    None,
                    format!("正在卸载技能 `{skill_id}`。"),
                );
                Task::perform(
                    luaskills_runtime::perform_luaskills_management_action(
                        handle,
                        LuaSkillsManagementAction::Uninstall { skill_id },
                    ),
                    Message::LuaSkillsManagementFinished,
                )
            }
            Message::SetLuaSkillToolVisibility {
                skill_id,
                local_name,
                hidden,
            } => {
                match self.persist_luaskills_tool_visibility(&skill_id, &local_name, hidden) {
                    Ok(true) => {
                        self.luaskills_notice = if hidden {
                            format!(
                                "已隐藏技能 `{skill_id}` 的工具 `{local_name}`，仅影响宿主对模型暴露。"
                            )
                        } else {
                            format!(
                                "已恢复技能 `{skill_id}` 的工具 `{local_name}`，宿主会重新向模型暴露它。"
                            )
                        };
                        self.status_message = self.luaskills_notice.clone();
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.luaskills_notice = format!("保存 LuaSkills 工具可见性失败：{error}");
                        self.status_message = self.luaskills_notice.clone();
                        self.push_activity(
                            ActivityLevel::Warning,
                            "保存 LuaSkills 工具可见性失败",
                            error,
                        );
                    }
                }

                Task::none()
            }
            Message::LuaSkillsManagementFinished(result) => {
                self.luaskills_operation_in_flight = false;
                self.luaskills_operation_progress = None;
                if let Some(handle) = self.luaskills_runtime.as_ref() {
                    let _ = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
                }

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
                self.refresh_runtime_foundation();

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
            Message::OpenAskDebugModal => {
                self.active_question_modal = Some(Self::build_ask_debug_question_modal_state());
                self.status_message =
                    String::from("已打开 ask 调试弹窗，可直接验证不同问题类型与样式。");
                self.sync_active_question_modal_focus()
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
                self.status_message = self.tf(
                    I18nKey::StatusMainPanelSwitched,
                    &[I18nArg::named(
                        "panel_label",
                        panel.localized_label(self.current_locale()),
                    )],
                );

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
            Message::ProjectRulesWatchChanged { workspace_path } => {
                crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_rules_prompt_cache(
                    &workspace_path,
                );
                crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                    vulcan_prompt_composer::PromptInvalidationReason::ProjectRuleFileChanged,
                );
                Task::none()
            }
            Message::PromptResourcesWatchChanged {
                workspace_path,
                kind,
            } => {
                match kind {
                    PromptResourceWatchKind::ProjectPrompt => {
                        crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_user_prompt_cache(
                            &workspace_path,
                            &self.provider_configuration.prompt.project_prompt_path,
                        );
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ProjectPromptFileChanged,
                        );
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message =
                            String::from("项目提示词已变化，提示词用户上下文缓存已刷新。");
                    }
                    PromptResourceWatchKind::ProjectPromptConfig => {
                        crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_user_prompt_cache(
                            &workspace_path,
                            &self.provider_configuration.prompt.project_prompt_path,
                        );
                        if self.project_prompt_settings_draft.workspace_path == workspace_path {
                            self.reload_project_prompt_settings_draft();
                        }
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ProjectPromptFileChanged,
                        );
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message =
                            String::from("项目提示词配置已变化，提示词用户上下文缓存已刷新。");
                    }
                    PromptResourceWatchKind::UserPromptResources => {
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::PersonalityChanged,
                        );
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ToolSchemaChanged,
                        );
                        match crate::agent_runtime::prompt_composer_runtime::reload_default_runtime_prompt_composer_engine() {
                            Ok(()) => {
                                self.refresh_prompt_composer_inspector_buffer();
                                self.status_message = String::from("用户级提示词资源已变化，运行时提示词组合器已刷新。");
                            }
                            Err(error) => {
                                self.status_message = format!("用户级提示词资源变化后刷新组合器失败：{error}");
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ComposerChanged(value) => {
                self.composer = value;
                self.refresh_composer_slash_command_state();
                self.save_selected_session_composer_draft();
                self.persist_workspace_snapshot_task()
            }
            Message::ComposerEditorAction(action) => {
                let task = self.apply_composer_editor_action(action);
                self.refresh_composer_slash_command_state();
                task
            }
            Message::ComposerSlashCommandMoveSelectionUp => {
                self.move_composer_slash_command_highlight(false);
                Task::none()
            }
            Message::ComposerSlashCommandMoveSelectionDown => {
                self.move_composer_slash_command_highlight(true);
                Task::none()
            }
            Message::ActivateComposerSlashCommand(command) => {
                self.activate_composer_slash_command(command)
            }
            Message::DismissComposerSlashCommandPanel => {
                self.dismiss_composer_slash_command_panel();
                Task::none()
            }
            Message::ComposerUndo => {
                let task = self.restore_latest_composer_undo_snapshot();
                self.refresh_composer_slash_command_state();
                task
            }
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
            Message::OpenAboutLicensesDirectory => {
                match self
                    .about_license_catalog
                    .preferred_directory_to_open()
                    .map(Path::to_path_buf)
                {
                    Some(directory) => {
                        match open_path_in_system_file_manager(&directory.display().to_string()) {
                            Ok(()) => {
                                self.status_message = String::from("已打开许可证资源目录。");
                            }
                            Err(error) => {
                                self.show_workbench_notice_modal(
                                    "无法打开许可证资源目录",
                                    error.clone(),
                                );
                                self.status_message = format!("打开许可证资源目录失败：{error}");
                            }
                        }
                    }
                    None => {
                        self.status_message =
                            String::from("当前运行环境还没有可打开的许可证资源目录。");
                    }
                }
                Task::none()
            }
            Message::OpenAboutLicenseDocumentPath(document_id) => {
                match self
                    .about_license_catalog
                    .document(&document_id)
                    .and_then(|document| document.external_path.as_ref())
                    .map(PathBuf::from)
                {
                    Some(path) => {
                        match open_path_with_system_default_application(&path.display().to_string())
                        {
                            Ok(()) => {
                                self.status_message = String::from("已打开许可证原始文件。");
                            }
                            Err(error) => {
                                self.show_workbench_notice_modal(
                                    "无法打开许可证原始文件",
                                    error.clone(),
                                );
                                self.status_message = format!("打开许可证原始文件失败：{error}");
                            }
                        }
                    }
                    None => {
                        self.status_message =
                            String::from("当前许可证文档没有可直接打开的本地文件。");
                    }
                }
                Task::none()
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
                if self.active_question_modal.is_some() {
                    self.sync_active_question_modal_focus()
                } else {
                    Task::none()
                }
            }
            Message::QuestionRuntimePollTick => {
                if self.sync_active_question_modal_from_runtime() {
                    self.sync_active_question_modal_focus()
                } else {
                    Task::none()
                }
            }
            Message::QuestionModalOptionPressed {
                question_index,
                option_index,
            } => {
                self.toggle_active_question_modal_option(question_index, option_index);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalCustomTextChanged {
                question_index,
                value,
            } => {
                self.update_active_question_modal_custom_answer(question_index, value);
                Task::none()
            }
            Message::QuestionModalSelectQuestionTab(question_index) => {
                self.select_active_question_modal_question(question_index);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalSelectConfirmTab => {
                self.open_active_question_modal_confirm_stage();
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalMoveToPreviousQuestion => {
                self.move_active_question_modal_question(false);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalMoveToNextQuestion => {
                self.move_active_question_modal_question(true);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalToggleHighlightedOption => {
                if self.active_question_modal_stage()
                    == Some(WorkbenchQuestionModalStage::Confirming)
                {
                    self.confirm_active_question_modal_page()
                } else {
                    self.toggle_active_question_modal_highlighted_option();
                    self.sync_active_question_modal_focus()
                }
            }
            Message::SubmitQuestionModal => self.submit_active_question_modal(),
            Message::DismissTopLevelOverlay => {
                if self.composer_slash_command_panel_state().is_some() {
                    self.dismiss_composer_slash_command_panel();
                    return Task::none();
                }
                if self.workbench_context_menu.take().is_some() {
                    self.composer_context_clipboard_text = None;
                    return Task::none();
                }
                if self.workbench_image_preview_modal.take().is_some() {
                    return Task::none();
                }
                if self.workbench_notice_modal.take().is_some() {
                    if self.active_question_modal.is_some() {
                        return self.sync_active_question_modal_focus();
                    }
                    return Task::none();
                }
                if self.active_question_modal.is_some() {
                    return self.request_active_question_modal_rejection();
                }
                if self.about_license_active_document_id.take().is_some() {
                    self.about_license_document_buffer_loading = false;
                    self.about_license_document_buffer_load_error = None;
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
                if self.active_question_modal.is_some() {
                    return self.confirm_active_question_modal_page();
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
            Message::ExamplePreviewMarkdownAction(action) => {
                if !action.is_edit() {
                    self.example_preview_markdown_buffer.perform(action);
                }
                Task::none()
            }
            Message::SettingsSectionSelected(section) => {
                let section_changed = self.selected_settings_section != section;
                self.selected_settings_section = section;
                if section.belongs_to_ai_capabilities() {
                    self.settings_ai_capability_tree_expanded = true;
                }
                if section.belongs_to_model_settings() {
                    self.settings_model_tree_expanded = true;
                }
                if section == SettingsSection::RuntimeDiagnostics {
                    self.refresh_prompt_composer_inspector_buffer();
                }
                if section == SettingsSection::PromptComposer {
                    self.reload_project_prompt_settings_draft();
                }
                self.status_message = self.tf(
                    I18nKey::StatusSettingsSectionSwitched,
                    &[I18nArg::named(
                        "section_title",
                        section.localized_title(self.current_locale()),
                    )],
                );
                if section == SettingsSection::AboutLicenses
                    && self.should_auto_start_about_license_catalog_reload(section_changed)
                {
                    self.start_about_license_catalog_reload(
                        AboutLicenseCatalogLoadTrigger::OpenSection,
                    )
                } else {
                    Task::none()
                }
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
            Message::OpenWorkbenchOpenRouterEndpointPicker => {
                self.open_workbench_openrouter_endpoint_picker()
            }
            Message::CloseWorkbenchOpenRouterEndpointPicker => {
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointPickerFilterChanged(value) => {
                self.workbench_openrouter_endpoint_picker.filter = value;
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointSelected(selection) => {
                let result = self.set_active_workbench_openrouter_endpoint_route(selection.clone());
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                match result {
                    Ok(()) => {
                        self.status_message = selection
                            .as_ref()
                            .map(|route| {
                                format!("已切换 OpenRouter 二级供应商：{}", route.provider_name)
                            })
                            .unwrap_or_else(|| String::from("已切换为 OpenRouter 自动路由。"));
                    }
                    Err(error) => {
                        self.status_message = error.clone();
                        self.show_workbench_notice_modal("保存二级供应商失败", error);
                    }
                }
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointsLoaded { model_id, result } => {
                if self.workbench_openrouter_endpoint_picker.model_id != model_id {
                    return Task::none();
                }
                self.workbench_openrouter_endpoint_picker.loading = false;
                match result {
                    Ok(choices) => {
                        self.workbench_openrouter_endpoint_picker.error = None;
                        self.workbench_openrouter_endpoint_picker.choices = choices;
                    }
                    Err(error) => {
                        self.workbench_openrouter_endpoint_picker.choices.clear();
                        self.workbench_openrouter_endpoint_picker.error = Some(error.clone());
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::OpenWorkbenchModelPicker => {
                self.dismiss_composer_slash_command_panel();
                self.open_workbench_model_picker()
            }
            Message::CloseWorkbenchModelPicker => {
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.workbench_model_picker_scroll_offset = 0.0;
                self.workbench_model_picker_viewport_height = WORKBENCH_MODEL_PICKER_LIST_HEIGHT;
                Task::none()
            }
            Message::OpenWorkbenchReasoningPicker => {
                self.dismiss_composer_slash_command_panel();
                self.workbench_model_picker_open = false;
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
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
                if self.active_question_modal.is_some() {
                    self.move_active_question_modal_option(false);
                    return Task::none();
                }
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
                if self.active_question_modal.is_some() {
                    self.move_active_question_modal_option(true);
                    return Task::none();
                }
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
                if self.project_attachment_cleanup_busy() {
                    self.status_message =
                        String::from("当前项目附件清理仍在处理中，请稍候再修改清理条件。");
                    return Task::none();
                }
                self.project_attachment_cleanup_cutoff_date = date;
                self.project_attachment_cleanup_date_picker_open = false;
                self.project_attachment_cleanup_preview = None;
                self.project_attachment_cleanup_notice =
                    String::from("项目附件清理截止日期已更新，可先预览再执行实际清理。");
                Task::none()
            }
            Message::ProjectAttachmentCleanupKindToggled(kind, enabled) => {
                if self.project_attachment_cleanup_busy() {
                    self.status_message =
                        String::from("当前项目附件清理仍在处理中，请稍候再修改清理条件。");
                    return Task::none();
                }
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
                if self.project_attachment_cleanup_busy() {
                    self.project_attachment_cleanup_notice =
                        String::from("当前项目附件清理仍在处理中，请稍候。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                let Some(context) = self.active_project_attachment_cleanup_context() else {
                    self.project_attachment_cleanup_notice =
                        String::from("无法预览清理范围：项目已经不存在。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                self.start_project_attachment_cleanup_preview(context)
            }
            Message::ProjectAttachmentCleanupPreviewFinished(context, result) => {
                self.project_attachment_cleanup_preview_in_flight = false;
                match result {
                    Ok(preview) => {
                        if self.project_attachment_cleanup_context_matches_current(&context) {
                            let preview_summary =
                                self.format_attachment_cleanup_preview_summary(&preview);
                            self.project_attachment_cleanup_preview = Some(preview);
                            self.project_attachment_cleanup_last_report = None;
                            self.project_attachment_cleanup_notice = format!(
                                "已生成项目 `{}` 的附件清理预览：{preview_summary}",
                                context.project_title
                            );
                            self.status_message = self.project_attachment_cleanup_notice.clone();
                        }
                    }
                    Err(error) => {
                        if self.project_attachment_cleanup_context_matches_current(&context) {
                            self.project_attachment_cleanup_preview = None;
                            self.project_attachment_cleanup_notice = error.clone();
                            self.status_message = error;
                        } else {
                            self.status_message = format!(
                                "项目 `{}` 的附件清理预览失败：{error}",
                                context.project_title
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::ExecuteProjectAttachmentCleanup => {
                if self.project_attachment_cleanup_busy() {
                    self.project_attachment_cleanup_notice =
                        String::from("当前项目附件清理仍在处理中，请稍候。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                }
                let Some(context) = self.active_project_attachment_cleanup_context() else {
                    self.project_attachment_cleanup_notice =
                        String::from("无法执行清理：项目已经不存在。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                let Some(preview) = self.project_attachment_cleanup_preview.clone() else {
                    self.project_attachment_cleanup_notice =
                        String::from("请先预览当前项目的附件清理范围，再执行实际清理。");
                    self.status_message = self.project_attachment_cleanup_notice.clone();
                    return Task::none();
                };
                if preview.cutoff_date_text != context.cutoff_date_text {
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
                let preview_summary = self.format_attachment_cleanup_preview_summary(&preview);
                self.show_workbench_confirmation_modal(
                    "确认清理项目附件缓存",
                    format!(
                        "将清理项目 `{}` 的历史附件缓存：{}\n\n清理后，旧会话中的这些附件会撤销回放，不再发送给模型。是否继续？",
                        context.project_title, preview_summary
                    ),
                    "继续清理",
                    "取消",
                    WorkbenchNoticeConfirmAction::ExecuteProjectAttachmentCleanup {
                        request: ProjectAttachmentCleanupExecutionRequest {
                            context,
                            expected_preview: preview,
                        },
                    },
                );
                Task::none()
            }
            Message::ProjectAttachmentCleanupExecutionFinished(context, result) => {
                self.project_attachment_cleanup_execution_in_flight = false;
                match result {
                    Ok(ProjectAttachmentCleanupExecutionOutcome::Executed {
                        report,
                        refreshed_preview,
                    }) => {
                        let report_summary = self.format_attachment_cleanup_report_summary(&report);
                        self.status_message = format!(
                            "项目 `{}` 的历史附件缓存清理已完成：{report_summary}",
                            context.project_title
                        );
                        if self.project_attachment_cleanup_project_matches_current(&context) {
                            self.project_attachment_cleanup_last_report = Some(report);
                            if self.project_attachment_cleanup_context_matches_current(&context) {
                                self.project_attachment_cleanup_preview = refreshed_preview;
                            }
                            self.project_attachment_cleanup_notice = self.status_message.clone();
                        }
                    }
                    Ok(ProjectAttachmentCleanupExecutionOutcome::PreviewChanged { preview }) => {
                        let preview_summary =
                            self.format_attachment_cleanup_preview_summary(&preview);
                        self.status_message = format!(
                            "项目 `{}` 的附件清理范围已经变化，已刷新预览：{preview_summary}。请确认后再次执行。",
                            context.project_title
                        );
                        if self.project_attachment_cleanup_project_matches_current(&context) {
                            self.project_attachment_cleanup_preview = Some(preview);
                            self.project_attachment_cleanup_notice = self.status_message.clone();
                        }
                    }
                    Err(error) => {
                        self.status_message =
                            format!("项目 `{}` 的附件清理失败：{error}", context.project_title);
                        if self.project_attachment_cleanup_project_matches_current(&context) {
                            self.project_attachment_cleanup_notice = error;
                        }
                    }
                }
                Task::none()
            }
            Message::ThemeSelected(theme) => {
                self.app_theme = theme.clone();
                self.rebuild_quick_guide_markdown_buffer();
                self.status_message = self.tf(
                    I18nKey::StatusThemeChanged,
                    &[I18nArg::named("theme_name", theme.to_string())],
                );
                Task::none()
            }
            Message::LocaleSelected(locale) => {
                self.language_manager.set_locale(locale);
                clear_selectable_transcript_render_cache_stash();
                self.rebuild_quick_guide_markdown_buffer();
                self.refresh_all_session_agent_view_localizations();
                match self.persist_locale_preference() {
                    Ok(()) => {
                        self.status_message = self.tf(
                            I18nKey::StatusLocaleChanged,
                            &[I18nArg::named("locale_name", locale.native_name())],
                        );
                    }
                    Err(error) => {
                        self.status_message = format!(
                            "{} {}",
                            self.tf(
                                I18nKey::StatusLocaleChanged,
                                &[I18nArg::named("locale_name", locale.native_name())],
                            ),
                            error
                        );
                    }
                }
                Task::none()
            }
            Message::SessionTranscriptTurnPageSizeSelected(turn_limit) => {
                if !SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS.contains(&turn_limit)
                    || self.session_transcript_turn_page_size == turn_limit
                {
                    return Task::none();
                }

                self.session_transcript_turn_page_size = turn_limit;
                match self.persist_session_transcript_turn_page_size_preference() {
                    Ok(()) => {
                        self.status_message =
                            format!("启动会话分页已切换为每页 {turn_limit} 个完整轮次。");
                    }
                    Err(error) => {
                        self.status_message = format!(
                            "启动会话分页已切换为每页 {turn_limit} 个完整轮次，但保存配置失败：{error}"
                        );
                    }
                }
                Task::none()
            }
            Message::CommandShellPreferenceSelected(choice) => {
                let current_raw = self
                    .provider_configuration
                    .default_command_shell
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_ascii_lowercase());
                let next_raw = choice.shell.map(|shell| shell.id().to_string());
                if current_raw == next_raw {
                    return Task::none();
                }

                match self.persist_default_command_shell_preference(choice.shell) {
                    Ok(status_message) => {
                        self.status_message = status_message;
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::PromptPersonalitySelected(choice) => {
                if self.provider_configuration.prompt.personality == choice.selection {
                    return Task::none();
                }
                self.provider_configuration.prompt.personality = choice.selection;
                match self.persist_prompt_configuration() {
                    Ok(()) => {
                        self.refresh_runtime_foundation();
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message = format!("提示词人格已切换为 {}。", choice.label);
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ProjectPromptInjectionToggled(enabled) => {
                if self.provider_configuration.prompt.project_prompt_enabled == enabled {
                    return Task::none();
                }
                self.provider_configuration.prompt.project_prompt_enabled = enabled;
                match self.persist_prompt_configuration() {
                    Ok(()) => {
                        self.refresh_runtime_foundation();
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message = if enabled {
                            String::from("已启用项目级用户提示词注入。")
                        } else {
                            String::from("已禁用项目级用户提示词注入。")
                        };
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ProjectPromptOverrideToggled(enabled) => {
                self.ensure_project_prompt_settings_draft_current();
                self.project_prompt_settings_draft.enabled = enabled;
                self.project_prompt_settings_draft.notice =
                    String::from("项目提示词覆盖已修改，点击保存后写入当前项目配置。");
                Task::none()
            }
            Message::ProjectPromptPathChanged(value) => {
                self.ensure_project_prompt_settings_draft_current();
                self.project_prompt_settings_draft.path = value;
                self.project_prompt_settings_draft.notice =
                    String::from("项目提示词路径已修改，点击保存后写入当前项目配置。");
                Task::none()
            }
            Message::ReloadProjectPromptConfiguration => {
                self.reload_project_prompt_settings_draft();
                self.status_message = self.project_prompt_settings_draft.notice.clone();
                Task::none()
            }
            Message::SaveProjectPromptConfiguration => {
                match self.save_project_prompt_settings_draft() {
                    Ok(()) => {
                        self.status_message = self.project_prompt_settings_draft.notice.clone();
                    }
                    Err(error) => {
                        self.project_prompt_settings_draft.notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::QuickGuideMarkdownAction(action) => {
                if !action.is_edit() {
                    self.quick_guide_markdown_buffer.perform(action);
                }
                Task::none()
            }
            Message::TogglePromptComposerInspectorLayer(layer) => {
                if !self
                    .prompt_composer_inspector_expanded_layers
                    .remove(&layer)
                {
                    self.prompt_composer_inspector_expanded_layers.insert(layer);
                    self.ensure_prompt_composer_inspector_layer_buffer(layer);
                }
                Task::none()
            }
            Message::PromptComposerInspectorLayerAction(layer, action) => {
                if !action.is_edit() {
                    if let Some(content) =
                        self.prompt_composer_inspector_layer_buffers.get_mut(&layer)
                    {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::TogglePromptComposerInspectorSegment(segment_key) => {
                if !self
                    .prompt_composer_inspector_expanded_segments
                    .remove(&segment_key)
                {
                    let opened_segment_key = segment_key.clone();
                    self.prompt_composer_inspector_expanded_segments
                        .insert(segment_key);
                    self.ensure_prompt_composer_inspector_segment_buffer(&opened_segment_key);
                }
                Task::none()
            }
            Message::PromptComposerInspectorSegmentAction(segment_key, action) => {
                if !action.is_edit() {
                    if let Some(content) = self
                        .prompt_composer_inspector_segment_buffers
                        .get_mut(&segment_key)
                    {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::CopyPromptComposerInspectorReport => {
                if self.prompt_composer_inspector_markdown.trim().is_empty() {
                    self.refresh_prompt_composer_inspector_buffer();
                }
                if self.prompt_composer_inspector_markdown.trim().is_empty() {
                    self.status_message = String::from("暂无可复制的提示词组合报告。");
                    Task::none()
                } else {
                    self.status_message = String::from("已复制完整提示词组合报告。");
                    clipboard::write(self.prompt_composer_inspector_markdown.clone()).discard()
                }
            }
            Message::CopyPromptComposerInspectorLayer(layer) => {
                self.copy_prompt_composer_inspector_layer(layer)
            }
            Message::CopyPromptComposerInspectorSegment(segment_key) => {
                self.copy_prompt_composer_inspector_segment(segment_key)
            }
            Message::AboutLicenseCatalogLoaded(revision, result) => {
                self.finish_about_license_catalog_reload(revision, result)
            }
            Message::ToggleAboutLicenseDocumentBody(document_id) => {
                if self.about_license_active_document_id.as_deref() == Some(document_id.as_str()) {
                    self.about_license_active_document_id = None;
                    self.about_license_document_buffer_loading = false;
                    self.about_license_document_buffer_load_error = None;
                    Task::none()
                } else {
                    self.start_about_license_document_buffer_load(document_id)
                }
            }
            Message::AboutLicenseDocumentBufferLoaded(revision, document_id, result) => {
                self.finish_about_license_document_buffer_load(revision, document_id, result)
            }
            Message::AboutLicenseDocumentRawAction(document_id, action) => {
                if !action.is_edit() {
                    if let Some(content) = self.about_license_document_buffers.get_mut(&document_id)
                    {
                        content.perform(action);
                    }
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
                let previous_workspace_path = self.selected_attachment_workspace_path();
                let previous_scope = self.composer_attachment_ref_scope();
                self.save_selected_session_composer_draft();
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
                self.reload_project_prompt_settings_draft();
                self.restore_session_composer_draft(&previous_workspace_path, &previous_scope);
                self.clear_transcript_drag_auto_scrolls();
                self.active_subagent_panel = None;
                self.rebuild_subagent_task_cache_for_session(session_id);
                self.sync_context_usage_from_session_budget(session_id);
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

                self.status_message = self.tf(
                    I18nKey::StatusSessionSwitched,
                    &[I18nArg::named("selected_title", selected_title.as_str())],
                );
                let projection_task =
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id);
                Task::batch([
                    self.persist_workspace_snapshot_task(),
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
                self.ensure_session_last_api_request_preview_buffer(session_id);
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
            Message::StopSelectedCommandSession => {
                let Some((owner_session_id, command_session_id)) =
                    self.active_command_session_modal_stop_target()
                else {
                    self.status_message = String::from("当前没有可停止的命令会话弹层。");
                    return Task::none();
                };
                let Some(modal_state) = self.active_tool_modal.as_mut() else {
                    self.status_message = String::from("当前没有可停止的命令会话弹层。");
                    return Task::none();
                };
                if modal_state.command_session_stop_in_flight {
                    self.status_message = String::from("命令会话停止请求已经发出，请稍候。");
                    return Task::none();
                }
                modal_state.command_session_stop_in_flight = true;
                let command_session_runtime = self.runtime_foundation.command_session_runtime();
                Task::perform(
                    async move {
                        command_session_runtime
                            .stop_session(owner_session_id, command_session_id, None)
                            .await
                    },
                    Message::SelectedCommandSessionStopFinished,
                )
            }
            Message::SelectedCommandSessionStopFinished(result) => {
                let active_modal_session_id = self
                    .active_tool_modal
                    .as_ref()
                    .map(|modal_state| modal_state.session_id);
                let command_tool_call_id = self.active_command_session_modal_tool_call_id();
                if let Some(modal_state) = self.active_tool_modal.as_mut() {
                    modal_state.command_session_stop_in_flight = false;
                    if let Ok(snapshot) = result.as_ref() {
                        modal_state
                            .payload
                            .tool_modal_presentation
                            .apply_command_session_snapshot(snapshot);
                    }
                }
                match result {
                    Ok(snapshot) => {
                        let tool_call_id = command_tool_call_id.unwrap_or_else(|| {
                            format!("gui-command-session-{}", snapshot.command_session_id)
                        });
                        let transcript_session_id =
                            active_modal_session_id.unwrap_or(snapshot.owner_session_id);
                        self.append_command_session_stop_result_message(
                            transcript_session_id,
                            &tool_call_id,
                            &snapshot,
                        );
                        self.status_message =
                            format!("命令会话 `{}` 已停止。", snapshot.command_session_id);
                    }
                    Err(error) => {
                        self.status_message = format!("停止命令会话失败：{error}");
                    }
                }
                Task::none()
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

                self.ensure_session_last_api_request_preview_buffer(session_id);
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
                    self.status_message = self.t(I18nKey::StatusOlderHistoryLoadingInProgress);
                    return Task::none();
                }
                if !self.session_history_has_more.contains(&session_id) {
                    self.status_message = self.t(I18nKey::StatusNoMoreOlderHistory);
                    return Task::none();
                }

                self.status_message = self.t(I18nKey::StatusOlderHistoryLoading);
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
                self.workspace_history_search.query =
                    sanitize_workspace_history_search_query_input(&query);
                self.workspace_history_search.error = None;
                if self.workspace_history_search.query.trim().is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.pending_navigation = None;
                }
                Task::none()
            }
            Message::SubmitWorkspaceHistorySearch => {
                if self.workspace_history_search.loading {
                    self.status_message = self.t(I18nKey::StatusHistorySearchInProgress);
                    return Task::none();
                }
                let query = self.workspace_history_search.query.trim();
                if query.is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.error = None;
                    self.status_message = self.t(I18nKey::StatusHistorySearchEmptyQuery);
                    return self.open_workspace_history_search_panel();
                }

                self.workspace_history_search.panel_open = true;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                self.status_message = self.t(I18nKey::StatusHistorySearchStarted);
                self.schedule_workspace_history_search(false)
            }
            Message::LoadMoreWorkspaceHistorySearchResults => {
                if self.workspace_history_search.loading {
                    self.status_message = self.t(I18nKey::StatusHistorySearchInProgress);
                    return Task::none();
                }
                let Some(current_page) = self.workspace_history_search.results.as_ref() else {
                    self.status_message = self.t(I18nKey::StatusHistorySearchNoMoreResults);
                    return Task::none();
                };
                if !current_page.has_more {
                    self.status_message = self.t(I18nKey::StatusHistorySearchAllLoaded);
                    return Task::none();
                }
                if self.workspace_history_search.query.trim() != current_page.query {
                    self.status_message = self.t(I18nKey::StatusHistorySearchQueryChanged);
                    return Task::none();
                }

                self.workspace_history_search.error = None;
                self.status_message = self.t(I18nKey::StatusHistorySearchLoadMoreStarted);
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
                            self.status_message = self.tf(
                                I18nKey::StatusHistorySearchLoadMoreCompleted,
                                &[
                                    I18nArg::named("hit_count", hit_count.to_string()),
                                    I18nArg::named("loaded_count", loaded_count.to_string()),
                                    I18nArg::named("total_hits", total_hits.to_string()),
                                ],
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
                            self.status_message = self.tf(
                                I18nKey::StatusHistorySearchCompleted,
                                &[I18nArg::named("total_hits", total_hits.to_string())],
                            );
                        }
                    }
                    Err(error) => {
                        self.workspace_history_search.error = Some(error.clone());
                        self.status_message = self.tf(
                            I18nKey::StatusHistorySearchFailed,
                            &[I18nArg::named("error", error.as_str())],
                        );
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
            Message::SessionRuntimeProgressTick => {
                self.runtime_progress_animation_frame =
                    self.runtime_progress_animation_frame.wrapping_add(1);
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
                        self.refresh_prompt_composer_inspector_buffer();
                        self.sync_context_usage_from_preparation(Some(&preparation));
                        self.update_session_runtime_identity_from_preparation(
                            preparation.session_id,
                            &preparation,
                        );
                        self.last_execution_preparation = Some(preparation);
                        self.status_message =
                            String::from("真实执行已开始，正在分离思考与输出并持续写入工作台。");
                        self.push_activity(ActivityLevel::Success, "执行准备已完成", detail);
                        Task::none()
                    }
                    ExecutionStreamEvent::RequestStarted { session_id } => {
                        self.mark_session_pending_provider_response(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
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
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.clear_session_pending_provider_response(session_id);
                        if is_provider_tool_boundary_runtime_event(&event) {
                            self.dock_streaming_thinking_message(session_id);
                        }
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
            Message::SessionPromptCompactionFinished {
                run_id,
                session_id,
                trigger,
                budget_note,
                plan,
                result,
            } => {
                let Some(active_session_id) = self.active_compaction_session_id_for_run_id(run_id)
                else {
                    return Task::none();
                };
                if active_session_id != session_id {
                    return Task::none();
                }
                let Some(active_state) = self.take_active_compaction_state(session_id) else {
                    return Task::none();
                };

                match result {
                    Ok(execution) => self.finish_session_prompt_compaction(
                        session_id,
                        trigger,
                        budget_note,
                        active_state.previous_phase,
                        execution,
                    ),
                    Err(error) => {
                        self.push_activity(
                            ActivityLevel::Warning,
                            "提示词压缩回退",
                            format!(
                                "会话 #{session_id} 的提示词式压缩未返回可用总结，已回退到宿主侧摘要：{error}"
                            ),
                        );
                        self.finish_session_prompt_compaction(
                            session_id,
                            trigger,
                            budget_note,
                            active_state.previous_phase,
                            PromptCompactionExecution {
                                compaction_result: CompactionSkillResult::from_legacy_plan(&plan),
                                fallback_used: true,
                                provider_request_count: 0,
                            },
                        )
                    }
                }
            }
            Message::DetachedSubagentTaskEvent { task_id, event } => match event {
                DetachedTaskRuntimeEvent::ChildExecution(event) => match event {
                    ExecutionStreamEvent::Prepared(_) => Task::none(),
                    ExecutionStreamEvent::RequestStarted { session_id } => {
                        self.mark_session_pending_provider_response(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
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
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.clear_session_pending_provider_response(session_id);
                        if is_provider_tool_boundary_runtime_event(&event) {
                            self.dock_streaming_thinking_message(session_id);
                        }
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

    /// Starts one panel-scoped LuaSkills operation and seeds a synthetic progress snapshot before runtime events arrive.
    /// 启动一次面板作用域内的 LuaSkills 操作，并在运行时事件到达前写入一条合成进度快照。
    fn begin_luaskills_panel_operation(
        &mut self,
        action_kind: LuaSkillsOperationVisualKind,
        skill_id: Option<String>,
        root_name: Option<String>,
        message: String,
    ) {
        if let Some(handle) = self.luaskills_runtime.as_ref() {
            let _ = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
        }
        self.luaskills_operation_progress = Some(LuaSkillsOperationProgressState::queued(
            action_kind,
            skill_id,
            root_name,
            message.clone(),
        ));
        self.luaskills_notice = message.clone();
        self.status_message = message;
    }

    /// Drains queued LuaSkills lifecycle progress events from the runtime handle and mirrors the newest snapshot into the panel state.
    /// 从运行时句柄中取出已排队的 LuaSkills 生命周期进度事件，并将最新快照镜像到面板状态。
    fn poll_luaskills_operation_progress(&mut self) {
        let Some(handle) = self.luaskills_runtime.as_ref() else {
            return;
        };
        let queued_events = luaskills_runtime::drain_luaskills_operation_progress_events(handle);
        if queued_events.is_empty() {
            return;
        }
        for event in queued_events {
            self.apply_luaskills_operation_progress_event(event);
        }
    }

    /// Applies one runtime-originated LuaSkills progress event to the panel-local snapshot and notices.
    /// 将一条来自运行时的 LuaSkills 进度事件应用到面板本地快照与提示文案。
    fn apply_luaskills_operation_progress_event(
        &mut self,
        event: RuntimeSkillOperationProgressEvent,
    ) {
        let action_kind = match event.action {
            SkillLifecycleAction::Install => LuaSkillsOperationVisualKind::Install,
            SkillLifecycleAction::Update => LuaSkillsOperationVisualKind::Update,
            SkillLifecycleAction::Reload => LuaSkillsOperationVisualKind::Refresh,
            SkillLifecycleAction::Uninstall => LuaSkillsOperationVisualKind::Uninstall,
            SkillLifecycleAction::Enable => LuaSkillsOperationVisualKind::Enable,
            SkillLifecycleAction::Disable => LuaSkillsOperationVisualKind::Disable,
        };
        let mut progress_state = self
            .luaskills_operation_progress
            .clone()
            .unwrap_or_else(|| {
                LuaSkillsOperationProgressState::queued(
                    action_kind,
                    event.skill_id.clone(),
                    event.root_name.clone(),
                    String::from("LuaSkills 管理操作正在执行。"),
                )
            });
        if progress_state.operation_id != event.operation_id {
            progress_state = LuaSkillsOperationProgressState::queued(
                action_kind,
                event.skill_id.clone(),
                event.root_name.clone(),
                String::from("LuaSkills 管理操作正在执行。"),
            );
        }
        progress_state.action_kind = action_kind;
        progress_state.apply_runtime_event(&event);
        self.luaskills_notice = progress_state.notice_text();
        self.status_message = self.luaskills_notice.clone();
        self.luaskills_operation_progress = Some(progress_state);
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

    /// Returns the current execution phase tracked for the provided session, defaulting to idle when no explicit state exists yet.
    /// 返回给定会话当前跟踪到的执行阶段；若尚无显式状态则默认视为空闲。
    fn session_run_phase_for(&self, session_id: SessionId) -> SessionRunPhase {
        self.session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Idle)
    }

    /// Persists the current composer draft into the selected session runtime UI state without mutating transcript history.
    /// 在不修改转录历史的前提下，将当前输入框草稿持久化到已选会话的运行时界面状态中。
    fn save_selected_session_composer_draft(&mut self) {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return;
        };
        let text = self.composer_buffer.text();
        let attachments = self.composer_attachments.clone();
        let scroll_offset = self.composer_scroll_offset.max(0.0);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.ui_state.composer_draft = SessionComposerDraftState {
            text,
            attachments,
            scroll_offset,
        };
        runtime_state.updated_at = current_unix_millis_text();
    }

    /// Restores the selected session draft into the shared composer surface while re-binding attachment leases to the new session scope.
    /// 将已选会话草稿恢复到共享输入表面，同时把附件租约重新绑定到新会话作用域。
    fn restore_session_composer_draft(
        &mut self,
        previous_workspace_path: &str,
        previous_scope: &str,
    ) {
        let next_workspace_path = self.selected_attachment_workspace_path();
        let next_scope = self.composer_attachment_ref_scope();
        let previous_attachments = self.composer_attachments.clone();
        self.release_staged_attachment_refs(
            previous_workspace_path,
            previous_scope,
            &previous_attachments,
            "输入框",
        );
        self.release_all_composer_undo_attachment_refs();

        let draft = self
            .selected_session()
            .and_then(|session| self.session_runtime_state(session.id))
            .map(|runtime_state| runtime_state.ui_state.composer_draft.clone())
            .unwrap_or_default();

        if !draft.attachments.is_empty()
            && let Err(error) = self.acquire_staged_attachment_refs(
                &next_workspace_path,
                &next_scope,
                &draft.attachments,
                "输入框",
            )
        {
            self.show_workbench_notice_modal("草稿附件无法恢复", error.clone());
            self.status_message = error;
            self.composer.clear();
            self.composer_buffer = text_editor::Content::with_text("");
            self.composer_attachments.clear();
            self.composer_scroll_offset = 0.0;
            self.composer_drag_hover_labels.clear();
            self.composer_scrollbar_pointer_y = None;
            self.composer_scrollbar_drag_grab_offset = None;
            self.dismiss_workbench_context_menu();
            self.refresh_composer_slash_command_state();
            return;
        }

        self.composer = draft.text.clone();
        self.composer_buffer = text_editor::Content::with_text(&draft.text);
        self.composer_attachments = draft.attachments;
        self.composer_scroll_offset = draft.scroll_offset.max(0.0);
        self.composer_drag_hover_labels.clear();
        self.composer_scrollbar_pointer_y = None;
        self.composer_scrollbar_drag_grab_offset = None;
        self.dismiss_workbench_context_menu();
        self.refresh_composer_slash_command_state();
    }

    /// Replaces the current composer text while preserving staged attachments, undo safety, and transient UI state.
    /// 在保留已暂存附件、撤销安全性与临时界面状态的前提下替换当前输入框文本。
    fn replace_composer_text_preserving_attachments(&mut self, next_text: impl Into<String>) {
        let next_text = next_text.into();
        self.push_composer_undo_snapshot();
        self.composer = next_text.clone();
        self.composer_buffer = text_editor::Content::with_text(&next_text);
        self.composer_drag_hover_labels.clear();
        self.composer_scroll_offset = 0.0;
        self.composer_scrollbar_pointer_y = None;
        self.composer_scrollbar_drag_grab_offset = None;
        self.dismiss_workbench_context_menu();
        self.refresh_composer_slash_command_state();
        self.save_selected_session_composer_draft();
    }

    /// Rebuilds the runtime-only slash-command suggestion state from the latest composer text.
    /// 根据最新提示词文本重建仅运行时存在的 slash 指令建议状态。
    fn refresh_composer_slash_command_state(&mut self) {
        self.composer_slash_command = build_composer_slash_command_state(&self.composer);
    }

    /// Returns whether any blocking overlay should suppress the inline slash-command suggestion panel.
    /// 返回是否存在应压制内联 slash 指令建议面板的阻塞式遮罩层。
    fn top_level_overlay_blocks_composer_slash_commands(&self) -> bool {
        self.workbench_notice_modal.is_some()
            || self.active_question_modal.is_some()
            || self.workbench_image_preview_modal.is_some()
            || self.ai_capability_vendor_picker.is_some()
            || self.ai_capability_vendor_editor.is_some()
            || self.ai_capability_target_picker.is_some()
            || self.ai_capability_routing_picker.is_some()
            || self.provider_editor.is_some()
            || self.provider_model_picker.is_some()
            || self.provider_model_override_editor.is_some()
            || self.mcp_server_editor_open
            || self.project_settings_project_id.is_some()
            || self.workbench_ai_capability_panel_open
            || self.workbench_model_picker_open
            || self.workbench_openrouter_endpoint_picker.open
            || self.workbench_reasoning_picker_open
            || self.workspace_history_search.panel_open
            || self.about_license_active_document_id.is_some()
            || self.workbench_context_menu.is_some()
    }

    /// Returns the active inline slash-command suggestion state when no blocking overlay has taken priority.
    /// 返回在没有阻塞式遮罩层抢占优先级时处于激活态的内联 slash 指令建议状态。
    fn composer_slash_command_panel_state(&self) -> Option<&ComposerSlashCommandState> {
        if self.top_level_overlay_blocks_composer_slash_commands() {
            return None;
        }

        self.composer_slash_command.as_ref()
    }

    /// Returns the currently highlighted slash-command suggestion when the inline panel is active.
    /// 返回内联面板激活时当前高亮的 slash 指令建议。
    fn highlighted_composer_slash_command(&self) -> Option<ComposerSlashCommand> {
        self.composer_slash_command_panel_state()?
            .highlighted_command()
    }

    /// Returns the directly activatable slash command when the composer text exactly matches one canonical trigger.
    /// 返回提示词文本与某个规范触发词完全匹配时可被直接激活的 slash 指令。
    fn exact_composer_slash_command(&self) -> Option<ComposerSlashCommand> {
        let state = self.composer_slash_command_panel_state()?;
        ComposerSlashCommand::from_exact_input(&state.raw_input)
    }

    /// Moves the highlighted slash-command suggestion within the inline panel and wraps around at list edges.
    /// 在内联面板内移动高亮 slash 指令建议，并在列表边缘处循环。
    fn move_composer_slash_command_highlight(&mut self, forward: bool) {
        let Some(state) = self.composer_slash_command.as_mut() else {
            return;
        };
        let command_count = state.visible_commands.len();
        if command_count == 0 {
            state.highlighted_index = None;
            return;
        }

        let next_index = match (state.highlighted_index, forward) {
            (Some(index), true) => (index + 1) % command_count,
            (Some(index), false) => index.checked_sub(1).unwrap_or(command_count - 1),
            (None, _) => 0,
        };
        state.highlighted_index = Some(next_index);
    }

    /// Dismisses the inline slash-command suggestion panel without mutating the current composer text.
    /// 在不修改当前提示词文本的前提下关闭内联 slash 指令建议面板。
    fn dismiss_composer_slash_command_panel(&mut self) {
        self.composer_slash_command = None;
    }

    /// Activates one slash command by clearing the composer text first and then dispatching the corresponding workbench action.
    /// 先清空输入框中的 slash 文本，再分发对应工作台动作，以激活某条 slash 指令。
    fn activate_composer_slash_command(&mut self, command: ComposerSlashCommand) -> Task<Message> {
        self.replace_composer_text_preserving_attachments(String::new());
        let command_task = match command {
            ComposerSlashCommand::Model => self.open_workbench_model_picker(),
            ComposerSlashCommand::Compact => {
                self.execute_manual_selected_session_history_compaction()
            }
            ComposerSlashCommand::Reasoning => Task::done(Message::OpenWorkbenchReasoningPicker),
        };
        let persist_task = self.persist_workspace_snapshot_task();

        Task::batch([command_task, persist_task])
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
        self.refresh_composer_slash_command_state();
        self.save_selected_session_composer_draft();
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
        self.refresh_composer_slash_command_state();
        self.save_selected_session_composer_draft();
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
            WorkbenchNoticeConfirmAction::AbandonQuestionModal => {
                self.reject_active_question_modal()
            }
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
            WorkbenchNoticeConfirmAction::RebuildMissingSession { session_id } => {
                self.rebuild_missing_session_after_confirmation(session_id)
            }
            WorkbenchNoticeConfirmAction::ExecuteProjectAttachmentCleanup { request } => {
                self.start_project_attachment_cleanup_execution(request)
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

    /// Rebuilds one session whose canonical event-store files are missing by clearing loaded payloads while preserving the existing title row.
    /// 通过清理已加载载荷并保留现有标题行，重建一条规范事件文件已丢失的会话。
    fn rebuild_missing_session_after_confirmation(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let session_title = self
            .sessions
            .iter()
            .chain(self.archived_sessions.iter())
            .find(|session| session.id == session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| format!("会话 {session_id}"));

        self.clear_session_transcript_view_state(session_id);
        if session_is_archived {
            self.archived_session_messages
                .insert(session_id, Vec::new());
            self.archived_session_compactions
                .insert(session_id, Vec::new());
            self.archived_session_runtime_state
                .insert(session_id, SessionRuntimeStateEntry::default());
            self.session_messages.remove(&session_id);
            self.session_compactions.remove(&session_id);
            self.session_runtime_state.remove(&session_id);
        } else {
            self.session_messages.insert(session_id, Vec::new());
            self.session_compactions.insert(session_id, Vec::new());
            self.session_runtime_state
                .insert(session_id, SessionRuntimeStateEntry::default());
            self.archived_session_messages.remove(&session_id);
            self.archived_session_compactions.remove(&session_id);
            self.archived_session_runtime_state.remove(&session_id);
        }
        self.loading_session_ids.remove(&session_id);
        self.loading_older_session_history_ids.remove(&session_id);
        self.session_history_earliest_ordinals.remove(&session_id);
        self.session_history_has_more.remove(&session_id);
        if self
            .workspace_history_search
            .results
            .as_ref()
            .is_some_and(|page| page.hits.iter().any(|hit| hit.session_id == session_id))
        {
            self.workspace_history_search.results = None;
            self.workspace_history_search.loading = false;
            self.workspace_history_search.loading_more = false;
        }
        self.dirty_session_payload_ids.insert(session_id);
        self.dirty_session_message_ids.insert(session_id);
        self.hydrated_session_ids.insert(session_id);
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        self.refresh_session_queued_steps(session_id);
        self.status_message =
            format!("会话 `{session_title}` 的正文文件已按新会话状态重建，并保留原标题。");

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
        ])
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
        let mut perf_trace = TranscriptPerfTrace::begin(
            "build_persistable_workspace_snapshot",
            format!(
                "visible_sessions={} archived_sessions={} dirty_payload={} dirty_messages={}",
                self.sessions.len(),
                self.archived_sessions.len(),
                self.dirty_session_payload_ids.len(),
                self.dirty_session_message_ids.len()
            ),
        );
        let mut phase_started_at = Instant::now();
        let visible_session_ids = self
            .task_projects
            .iter()
            .flat_map(|project| project.session_ids.iter().copied())
            .collect::<BTreeSet<_>>();
        perf_trace.checkpoint("collect_visible_session_ids", phase_started_at);
        phase_started_at = Instant::now();
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
        perf_trace.checkpoint("build_session_sets", phase_started_at);
        phase_started_at = Instant::now();
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
        perf_trace.checkpoint("resolve_dirty_sets", phase_started_at);
        phase_started_at = Instant::now();

        let snapshot = WorkspaceSnapshot {
            task_projects: self.task_projects.clone(),
            archived_task_projects: self.archived_task_projects.clone(),
            sessions: visible_sessions,
            archived_sessions: archived_sessions.clone(),
            session_messages: self
                .session_messages
                .iter()
                .filter(|(session_id, _)| visible_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_message_ids.contains(session_id))
                .filter(|(session_id, _)| !self.session_runtime_state.contains_key(session_id))
                .map(|(session_id, messages)| (*session_id, messages.clone()))
                .filter(|(_, messages)| !messages.is_empty())
                .collect(),
            archived_session_messages: self
                .archived_session_messages
                .iter()
                .filter(|(session_id, _)| archived_session_ids.contains(session_id))
                .filter(|(session_id, _)| replaceable_session_message_ids.contains(session_id))
                .filter(|(session_id, _)| {
                    !self.archived_session_runtime_state.contains_key(session_id)
                })
                .map(|(session_id, messages)| (*session_id, messages.clone()))
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
        };
        perf_trace.checkpoint("clone_snapshot_payloads", phase_started_at);
        perf_trace.finish();
        snapshot
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
        self.active_runs.clear();
        self.active_session_compactions.clear();
        self.pending_provider_response_sessions.clear();
        self.runtime_progress_animation_frame = 0;
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
        self.pending_provider_response_sessions.remove(&session_id);
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
        self.session_last_api_request_markdowns.remove(&session_id);
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
                let title = human_tool_display_title(&tool_name, "running");
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
                let display_state = tool_display_state_for_message(
                    SessionMessageRecordKind::ToolResultOutput,
                    event.metadata.as_ref(),
                );
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
                                    let tool_name = message
                                        .metadata
                                        .as_ref()
                                        .and_then(|metadata| metadata.get("tool_name"))
                                        .and_then(Value::as_str)
                                        .filter(|tool_name| !tool_name.trim().is_empty());
                                    tool_name
                                        .map(|tool_name| {
                                            human_tool_display_title(tool_name, display_state)
                                        })
                                        .unwrap_or_else(|| {
                                            let call_title = message.title.trim();
                                            if call_title.is_empty() {
                                                human_tool_display_title(
                                                    &tool_call_id,
                                                    display_state,
                                                )
                                            } else {
                                                call_title.to_string()
                                            }
                                        })
                                })
                        })
                    })
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| human_tool_display_title(&tool_call_id, display_state));
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

        if self.session_owns_active_transcript_surface(session_id) {
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
        } else {
            self.session_runtime_events
                .entry(session_id)
                .or_default()
                .push(event);
            self.invalidate_hidden_session_streaming_view_caches(session_id, "");
        }
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
}

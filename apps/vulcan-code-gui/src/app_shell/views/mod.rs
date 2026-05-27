//! View composition methods extracted from the main workbench module.
//! 从主工作台模块中抽取的界面组合方法。

use super::helpers::*;
use super::*;
use crate::ui::widgets::context_usage_orb::{self, ContextUsageOrbProps, ContextUsageOrbTone};
use iced::widget::{column, opaque, pin, rich_text};
use iced::{Background, Border, Color};
use iced_aw::helpers::date_picker;
use vulcan_context::{ContextBudgetSeverity, ContextBudgetSnapshot, ContextLayerKind};

mod about_licenses;
mod ai_capability;
mod composer;
mod diff;
mod project_settings;
mod question_modal;
mod settings_cards;
mod settings_navigation;
mod task_tree;
mod transcript;

/// Minimum delay used to coalesce project-rules filesystem bursts.
/// 用于合并项目规则文件系统突发事件的最小延迟。
const PROJECT_RULES_WATCH_THROTTLE_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);
/// Minimum delay used to coalesce prompt-resource filesystem bursts.
/// 用于合并提示词资源文件系统突发事件的最小延迟。
const PROMPT_RESOURCE_WATCH_THROTTLE_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

/// Returns the process-wide live file watcher shared by all GUI filesystem subscriptions.
/// 返回供全部 GUI 文件系统订阅复用的进程级实时文件监听器。
fn shared_live_file_watcher() -> Option<std::sync::Arc<vulcan_file_watcher::live::LiveFileWatcher>>
{
    static SHARED_LIVE_FILE_WATCHER: std::sync::OnceLock<
        Option<std::sync::Arc<vulcan_file_watcher::live::LiveFileWatcher>>,
    > = std::sync::OnceLock::new();

    SHARED_LIVE_FILE_WATCHER
        .get_or_init(|| {
            vulcan_file_watcher::live::LiveFileWatcher::new()
                .ok()
                .map(std::sync::Arc::new)
        })
        .clone()
}

/// Builds a compact preview for one prompt-composer inspector layer.
/// 为单个提示词组合检查器分层构建紧凑预览。
fn prompt_composer_inspector_layer_preview(text: &str) -> String {
    let preview = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if preview.trim().is_empty() {
        return String::from("（空）");
    }
    if preview.chars().count() <= 180 {
        return preview;
    }
    format!("{}…", preview.chars().take(180).collect::<String>())
}

/// Formats one prompt-composer fingerprint for compact diagnostic display.
/// 为紧凑诊断展示格式化一个提示词组合指纹。
fn prompt_composer_inspector_fingerprint(fingerprint: u64) -> String {
    format!("{fingerprint:016x}")
}

/// Extracts one short summary preview from compaction metadata or the summary body as a fallback.
/// 从压缩元数据或摘要正文中提取一条短预览，元数据缺失时再回退正文。
fn composer_context_anchor_summary_preview(message: &SessionMessageEntry) -> String {
    if let Some(preview) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("preview"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|preview| !preview.is_empty())
    {
        return preview.to_string();
    }

    let summary_body = message
        .body
        .split_once("Earlier context summary:\n")
        .map(|(_, summary)| summary)
        .unwrap_or_else(|| message.body.as_str());
    let summary_body = summary_body
        .split_once("\n\nContinue from the newer messages below")
        .map(|(summary, _)| summary)
        .unwrap_or(summary_body);
    crate::agent_runtime::provider_stream_projection::truncate_preview(summary_body, 64)
}

/// Builds one compact compaction-anchor row from the referenced summary message.
/// 根据所引用的摘要消息构建一条紧凑的压缩锚点行。
fn composer_context_anchor_reference_row(
    message_id: &str,
    message: &SessionMessageEntry,
) -> String {
    let summary_preview = composer_context_anchor_summary_preview(message);
    if summary_preview.trim().is_empty() || summary_preview == "（空）" {
        format!("压缩锚点：已建立 · 关联消息 {}", message_id)
    } else {
        format!("压缩锚点：已建立 · {}", summary_preview)
    }
}

/// Stable input used to identify one project-rules watch subscription.
/// 用于标识单个项目规则监听订阅的稳定输入。
#[derive(Clone, Debug, Hash)]
struct ProjectRulesWatchSubscriptionInput {
    /// Workspace directory whose default project rules should be watched.
    /// 需要监听默认项目规则的工作区目录。
    workspace_path: String,
}

/// Stable input used to identify prompt-resource watch subscriptions.
/// 用于标识提示词资源监听订阅的稳定输入。
#[derive(Clone, Debug, Hash)]
struct PromptResourceWatchSubscriptionInput {
    /// Workspace directory whose project prompt should be watched.
    /// 需要监听项目提示词的工作区目录。
    workspace_path: String,
    /// Workspace-relative project prompt file path.
    /// 工作区相对项目提示词文件路径。
    project_prompt_path: String,
    /// Whether project prompt watching is enabled.
    /// 是否启用项目提示词监听。
    project_prompt_enabled: bool,
}

/// Returns the active subagent dispatch list height needed for the current running task count.
/// 返回当前运行中任务数量所需的活动子代理调度列表高度。
fn subagent_navigation_panel_height(task_count: usize) -> f32 {
    let row_count = task_count.max(1) as f32;
    let gap_count = task_count.saturating_sub(1) as f32;
    let content_height = SUBAGENT_NAVIGATION_PANEL_VERTICAL_PADDING
        + (row_count * SUBAGENT_NAVIGATION_ROW_HEIGHT)
        + (gap_count * SUBAGENT_NAVIGATION_ROW_GAP);

    content_height.max(SUBAGENT_NAVIGATION_PANEL_HEIGHT)
}

/// Splits one workspace-history preview markup string into ordered plain/highlight segments for rich-text rendering.
/// 将一段工作台历史预览标记字符串拆分为有序的普通片段与高亮片段，以供富文本渲染。
fn workspace_history_search_preview_segments(
    preview_markup: &str,
    fallback_preview: &str,
) -> Vec<(String, bool)> {
    let source = if preview_markup.trim().is_empty() {
        fallback_preview
    } else {
        preview_markup
    };
    let mut remaining = source;
    let mut segments = Vec::new();

    while let Some(start_index) = remaining.find("<mark>") {
        let (before_mark, after_mark_start) = remaining.split_at(start_index);
        if !before_mark.is_empty() {
            segments.push((before_mark.to_string(), false));
        }

        let marked_body = &after_mark_start["<mark>".len()..];
        if let Some(end_index) = marked_body.find("</mark>") {
            let (marked_text, after_mark_end) = marked_body.split_at(end_index);
            if !marked_text.is_empty() {
                segments.push((marked_text.to_string(), true));
            }
            remaining = &after_mark_end["</mark>".len()..];
        } else {
            if !marked_body.is_empty() {
                segments.push((marked_body.to_string(), true));
            }
            remaining = "";
        }
    }

    if !remaining.is_empty() {
        segments.push((remaining.to_string(), false));
    }

    if segments.is_empty() {
        segments.push((fallback_preview.to_string(), false));
    }

    segments
}

/// Builds the rich-text preview widget used by workspace-history search hits, preserving controller-provided highlight tags.
/// 构建工作台历史搜索命中使用的富文本预览部件，保留控制器提供的高亮标记效果。
fn workspace_history_search_preview_rich_text<'a>(
    theme: &Theme,
    preview_markup: &str,
    fallback_preview: &str,
) -> Element<'a, Message> {
    let palette = theme.extended_palette();
    let highlight_background = palette.primary.weak.color.scale_alpha(0.82);
    let highlight_border = Border {
        radius: 4.0.into(),
        width: 0.0,
        color: Color::TRANSPARENT,
    };
    let spans: Vec<iced::widget::text::Span<'static, (), iced::Font>> =
        workspace_history_search_preview_segments(preview_markup, fallback_preview)
            .into_iter()
            .map(|(text_segment, highlighted)| {
                let base = iced::widget::text::Span::<'_, (), iced::Font>::new(text_segment).color(
                    if highlighted {
                        palette.primary.strong.text
                    } else {
                        palette.background.base.text
                    },
                );
                if highlighted {
                    base.background(Background::Color(highlight_background))
                        .border(highlight_border)
                        .padding([0, 2])
                        .to_static()
                } else {
                    base.to_static()
                }
            })
            .collect::<Vec<_>>();

    rich_text(spans).size(12).width(Fill).into()
}

/// Builds a live watcher subscription for the active workspace `AGENTS.md`.
/// 为当前工作区的 `AGENTS.md` 构建实时监听订阅。
fn project_rules_watch_subscription(workspace_path: &str) -> Subscription<Message> {
    let workspace_path = workspace_path.trim();
    if workspace_path.is_empty() || !std::path::Path::new(workspace_path).is_dir() {
        return Subscription::none();
    }

    let workspace_path = workspace_path.to_string();
    Subscription::run_with(
        ProjectRulesWatchSubscriptionInput { workspace_path },
        project_rules_watch_stream,
    )
}

/// Builds the asynchronous stream backing one project-rules watch subscription.
/// 构建单个项目规则监听订阅背后的异步流。
fn project_rules_watch_stream(
    input: &ProjectRulesWatchSubscriptionInput,
) -> futures_util::stream::BoxStream<'static, Message> {
    let workspace_path = input.workspace_path.clone();
    Box::pin(async_stream::stream! {
        let target = vulcan_file_watcher::adapters::project_rules::ProjectRulesWatchTarget::workspace_default(
            std::path::PathBuf::from(&workspace_path),
        );

        if let Some(watcher) = shared_live_file_watcher() {
            let (subscriber, rx) = watcher.add_subscriber();
            let _registration = subscriber.register_paths(vec![target.to_live_watch_path()]);
            let mut rx = vulcan_file_watcher::live::ThrottledLiveWatchReceiver::new(
                rx,
                PROJECT_RULES_WATCH_THROTTLE_INTERVAL,
            );

            while let Some(event) = rx.recv().await {
                if event.paths.iter().any(|path| target.matches_path(path)) {
                    yield Message::ProjectRulesWatchChanged {
                        workspace_path: workspace_path.clone(),
                    };
                }
            }
        }
    })
}

/// Builds a live watcher subscription for prompt resource directories.
/// 为提示词资源目录构建实时监听订阅。
fn prompt_resource_watch_subscription(
    workspace_path: &str,
    project_prompt_path: &str,
    project_prompt_enabled: bool,
) -> Subscription<Message> {
    Subscription::run_with(
        PromptResourceWatchSubscriptionInput {
            workspace_path: workspace_path.trim().to_string(),
            project_prompt_path: project_prompt_path.trim().to_string(),
            project_prompt_enabled,
        },
        prompt_resource_watch_stream,
    )
}

/// Builds the asynchronous stream backing prompt-resource watching.
/// 构建提示词资源监听背后的异步流。
fn prompt_resource_watch_stream(
    input: &PromptResourceWatchSubscriptionInput,
) -> futures_util::stream::BoxStream<'static, Message> {
    let workspace_path = input.workspace_path.clone();
    let project_prompt_path = input.project_prompt_path.clone();
    let project_prompt_enabled = input.project_prompt_enabled;
    Box::pin(async_stream::stream! {
        let user_prompt_root = dirs::home_dir()
            .map(|home_dir| home_dir.join(".vulcan").join("code").join("prompt"));
        let target = vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchTarget::new(
            std::path::PathBuf::from(&workspace_path),
            project_prompt_path,
            project_prompt_enabled,
            user_prompt_root,
        );
        let watch_paths = target.to_live_watch_paths();
        if watch_paths.is_empty() {
            return;
        }

        if let Some(watcher) = shared_live_file_watcher() {
            let (subscriber, rx) = watcher.add_subscriber();
            let _registration = subscriber.register_paths(watch_paths);
            let mut rx = vulcan_file_watcher::live::ThrottledLiveWatchReceiver::new(
                rx,
                PROMPT_RESOURCE_WATCH_THROTTLE_INTERVAL,
            );

            while let Some(event) = rx.recv().await {
                let kind = prompt_resource_watch_kind_from_file_watcher(
                    target.classify_paths(&event.paths),
                );
                yield Message::PromptResourcesWatchChanged {
                    workspace_path: workspace_path.clone(),
                    kind,
                };
            }
        }
    })
}

/// Converts reusable file-watcher prompt-resource categories into GUI cache-invalidation categories.
/// 将可复用文件监听器提示词资源类别转换为 GUI 缓存失效类别。
fn prompt_resource_watch_kind_from_file_watcher(
    kind: vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchKind,
) -> PromptResourceWatchKind {
    match kind {
        vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchKind::ProjectPrompt => {
            PromptResourceWatchKind::ProjectPrompt
        }
        vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchKind::ProjectConfiguration => {
            PromptResourceWatchKind::ProjectPromptConfig
        }
        vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchKind::UserPromptResources => {
            PromptResourceWatchKind::UserPromptResources
        }
    }
}

impl VulcanWorkbench {
    /// Builds one unified two-line trigger used by the composer control row for model, AI capability, and reasoning selectors.
    /// 构建一类统一的双行触发器，供输入区控制行中的模型、AI 能力与思考级别选择器复用。
    fn view_workbench_dual_line_trigger<'a>(
        &self,
        icon: Option<Element<'a, Message>>,
        title: String,
        subtitle: String,
        width: f32,
        active: bool,
        on_press: Option<Message>,
    ) -> Element<'a, Message> {
        let palette = self.app_theme.extended_palette();
        let centered_title_column: Element<'_, Message> = container(
            column![
                container(
                    text(title)
                        .size(13)
                        .color(if active {
                            Color::WHITE
                        } else {
                            palette.background.base.text.scale_alpha(0.88)
                        })
                        .wrapping(iced::advanced::text::Wrapping::None),
                )
                .height(14)
                .width(Fill)
                .clip(true)
                .center_x(Fill),
                container(
                    text(subtitle)
                        .size(11)
                        .color(palette.background.base.text.scale_alpha(0.56))
                        .wrapping(iced::advanced::text::Wrapping::None),
                )
                .height(11)
                .width(Fill)
                .clip(true)
                .center_x(Fill),
            ]
            .spacing(2)
            .align_x(iced::Alignment::Center)
            .width(Fill),
        )
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into();
        let mut accessory_row = row![].align_y(Center).width(Fill);
        if let Some(icon) = icon {
            accessory_row = accessory_row.push(container(icon).width(18).center_y(Fill));
        }
        accessory_row = accessory_row.push(space::horizontal()).push(
            container(
                codicon::chevron_down()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.48)),
            )
            .width(14)
            .center_y(Fill),
        );
        let trigger_body = stack([
            centered_title_column,
            container(accessory_row)
                .width(Fill)
                .height(Fill)
                .center_y(Fill)
                .into(),
        ]);
        let trigger = button(container(trigger_body).width(Fill).height(Fill))
            .width(width)
            .height(COMPOSER_CONTROL_ROW_HEIGHT)
            .padding([0, 12])
            .style(style::workbench_model_trigger_button_style);
        if let Some(message) = on_press {
            trigger.on_press(message).into()
        } else {
            trigger.into()
        }
    }

    /// Builds the full window view by composing header, panes, and footer.
    /// 通过组合头部、分栏和底部状态栏构建完整窗口视图。
    pub(super) fn view(&self, window_id: window::Id) -> Element<'_, Message> {
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
    pub(super) fn view_main_window(&self, window_id: window::Id) -> Element<'_, Message> {
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

        let base_surface = self.wrap_with_window_resize_handles(window_id, main_surface);
        let mut layered_surfaces: Vec<Element<'_, Message>> = vec![base_surface];

        if let Some(overlay) = self.view_workbench_model_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_openrouter_endpoint_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_reasoning_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_ai_capability_vendor_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_ai_capability_vendor_editor_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_ai_capability_target_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_ai_capability_routing_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_provider_editor_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_mcp_server_editor_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_provider_model_picker_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_provider_model_override_editor_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_ai_capability_panel_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_context_menu_overlay() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workspace_history_search_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_task_project_settings_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_about_license_document_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_startup_overlay() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_question_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_notice_modal() {
            layered_surfaces.push(overlay);
        }
        if let Some(overlay) = self.view_workbench_image_preview_modal() {
            layered_surfaces.push(overlay);
        }

        stack(layered_surfaces).into()
    }

    /// Builds the framed main display region with dedicated top, left, and bottom separators.
    /// 构建带独立上侧、左侧、下侧分隔线的主显示区域。
    pub(super) fn view_main_display_region(&self) -> Element<'_, Message> {
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
    pub(super) fn view_startup_overlay(&self) -> Option<Element<'_, Message>> {
        match &self.startup_lifecycle {
            StartupLifecycle::Ready => None,
            StartupLifecycle::Bootstrapping { message } => {
                let backdrop = mouse_area(
                    container(space())
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style),
                )
                .on_press(Message::Noop)
                .on_right_press(Message::Noop);
                let loading_card = container(
                    column![
                        text(self.t(I18nKey::StartupLoadingTitle)).size(26),
                        text(self.t(I18nKey::StartupLoadingSubtitle)).size(14),
                        text(message).size(13),
                    ]
                    .spacing(10),
                )
                .padding([22, 26])
                .width(420)
                .style(style::startup_overlay_card_style);

                Some(
                    iced::widget::opaque(
                        stack([
                            backdrop.into(),
                            container(loading_card)
                                .width(Fill)
                                .height(Fill)
                                .center(Fill)
                                .into(),
                        ])
                        .width(Fill)
                        .height(Fill),
                    )
                    .into(),
                )
            }
            StartupLifecycle::Failed { message } => {
                let backdrop = mouse_area(
                    container(space())
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style),
                )
                .on_press(Message::Noop)
                .on_right_press(Message::Noop);
                let failure_card = container(
                    column![
                        text(self.t(I18nKey::StartupDataPlaneInitFailedTitle)).size(24),
                        text(message).size(13),
                        button(text(self.t(I18nKey::CommonRetry)))
                            .style(button::primary)
                            .on_press(Message::BeginStartupBootstrap),
                    ]
                    .spacing(14),
                )
                .padding([22, 26])
                .width(460)
                .style(style::startup_overlay_card_style);

                Some(
                    iced::widget::opaque(
                        stack([
                            backdrop.into(),
                            container(failure_card)
                                .width(Fill)
                                .height(Fill)
                                .center(Fill)
                                .into(),
                        ])
                        .width(Fill)
                        .height(Fill),
                    )
                    .into(),
                )
            }
        }
    }

    /// Builds the in-app workbench model-picker modal shown above the main workbench when model switching is requested.
    /// 构建在请求切换模型时显示在主工作台之上的内嵌工作台模型选择弹窗。
    pub(super) fn view_workbench_model_picker_modal(&self) -> Option<Element<'_, Message>> {
        if !self.workbench_model_picker_open {
            return None;
        }

        let filtered_choices = self.filtered_workbench_model_choices();
        let highlighted_index = self.workbench_model_picker_highlighted_index;
        let palette = self.app_theme.extended_palette();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseWorkbenchModelPicker);
        let option_list: Element<'_, Message> = if filtered_choices.is_empty() {
            container(
                container(text(self.t(I18nKey::WorkbenchModelPickerNoMatches)).size(14))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKBENCH_MODEL_PICKER_LIST_HEIGHT)
            .into()
        } else {
            let (start, end, top_spacer_height, bottom_spacer_height) =
                self.visible_workbench_model_picker_range(filtered_choices.len());
            scrollable(
                container(column![
                    space::vertical().height(top_spacer_height),
                    column(filtered_choices[start..end].iter().enumerate().map(
                        |(offset, choice)| {
                            let index = start + offset;
                            let is_selected = highlighted_index == Some(index);
                            let indicator: Element<'_, Message> = if is_selected {
                                codicon::chevron_right()
                                    .size(14)
                                    .line_height(1.0)
                                    .color(palette.primary.base.color)
                                    .into()
                            } else {
                                text("").size(14).into()
                            };
                            button(
                                container(
                                    row![
                                        container(indicator).width(18).center_y(Fill),
                                        container(text(choice.model_name.clone()).size(14))
                                            .width(Fill)
                                            .clip(true),
                                        container(
                                            text(choice.provider_label.clone()).size(13).color(
                                                palette.background.base.text.scale_alpha(0.56),
                                            ),
                                        )
                                        .padding(
                                            iced::Padding {
                                                top: 0.0,
                                                right: 28.0,
                                                bottom: 0.0,
                                                left: 0.0,
                                            }
                                        ),
                                    ]
                                    .spacing(10)
                                    .align_y(Center),
                                )
                                .width(Fill)
                                .center_y(Fill),
                            )
                            .width(Fill)
                            .height(WORKBENCH_MODEL_PICKER_ROW_HEIGHT)
                            .padding([0, 12])
                            .style(move |theme, status| {
                                style::workbench_model_picker_option_button_style(
                                    theme,
                                    status,
                                    is_selected,
                                )
                            })
                            .on_press(Message::WorkbenchModelSelected(choice.clone()))
                            .into()
                        },
                    ))
                    .spacing(WORKBENCH_MODEL_PICKER_ROW_GAP),
                    space::vertical().height(bottom_spacer_height),
                ])
                .padding(iced::Padding {
                    top: 0.0,
                    right: 14.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
            )
            .id(workbench_model_picker_scrollable_id())
            .on_scroll(Message::WorkbenchModelPickerScrolled)
            .height(WORKBENCH_MODEL_PICKER_LIST_HEIGHT)
            .into()
        };
        let modal_card = container(
            column![
                row![
                    text(self.t(I18nKey::WorkbenchModelPickerTitle)).size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text_input(
                    &self.t(I18nKey::WorkbenchModelPickerFilterPlaceholder),
                    &self.workbench_model_picker_filter
                )
                .id(WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID)
                .on_input(Message::WorkbenchModelPickerFilterChanged)
                .on_submit(Message::ConfirmTopLevelOverlay)
                .padding([10, 12]),
                option_list,
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(WORKBENCH_MODEL_PICKER_MODAL_WIDTH)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchModelPicker);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }

    /// Builds the blocking compact reasoning-effort picker modal opened from the composer footer trigger.
    /// 构建从输入区底部触发器打开的阻塞式紧凑思考级别选择弹窗。
    pub(super) fn view_workbench_reasoning_picker_modal(&self) -> Option<Element<'_, Message>> {
        if !self.workbench_reasoning_picker_open {
            return None;
        }

        let reasoning_view = self.active_workbench_reasoning_view();
        let reasoning_options = reasoning_view
            .as_ref()
            .map(|view| view.options.clone())
            .unwrap_or_else(|| {
                vec![provider_config::ReasoningSelectionOption {
                    effort: self.workbench_reasoning_effort,
                    label: self.workbench_reasoning_effort.compact_label().to_string(),
                }]
            });
        let selected_effort = reasoning_view
            .as_ref()
            .map(|view| view.selected_effort)
            .unwrap_or(self.workbench_reasoning_effort);
        let option_row_height = 30.0f32;
        let option_gap = 2.0f32;
        let panel_width = WORKBENCH_REASONING_SELECTOR_WIDTH.max(112.0);
        let option_list = column(reasoning_options.iter().cloned().map(|option| {
            let is_selected = option.effort == selected_effort;
            button(
                container(text(option.label.clone()).size(13).width(Fill))
                    .width(Fill)
                    .center_y(Fill),
            )
            .width(Fill)
            .height(option_row_height)
            .padding([0, 8])
            .style(move |theme, status| {
                style::workbench_model_picker_option_button_style(theme, status, is_selected)
            })
            .on_press(Message::WorkbenchReasoningEffortSelected(option))
            .into()
        }))
        .spacing(option_gap);
        let panel_height = (reasoning_options.len() as f32 * option_row_height)
            + (reasoning_options.len().saturating_sub(1) as f32 * option_gap)
            + 8.0;
        let show_explicit_stop_button = self.selected_session_has_busy_execution_lane()
            && self.composer_has_submittable_content();
        let composer_panel_width = self.composer_panel_width();
        let composer_panel_left = self.composer_panel_left();
        let trailing_controls_width = if show_explicit_stop_button {
            WORKBENCH_REASONING_SELECTOR_WIDTH
                + (COMPOSER_TRIGGER_BUTTON_SIZE * 2.0)
                + (COMPOSER_PANEL_CONTROL_GAP * 2.0)
        } else {
            WORKBENCH_REASONING_SELECTOR_WIDTH
                + COMPOSER_TRIGGER_BUTTON_SIZE
                + COMPOSER_PANEL_CONTROL_GAP
        };
        let panel_x = composer_panel_left + composer_panel_width
            - COMPOSER_PANEL_EDGE_PADDING as f32
            - trailing_controls_width;
        let status_bar_height = 44.0f32;
        let main_canvas_bottom_padding = 16.0f32;
        let panel_y = (self.main_window_size.height
            - status_bar_height
            - main_canvas_bottom_padding
            - COMPOSER_PANEL_EDGE_PADDING as f32
            - COMPOSER_CONTROL_ROW_HEIGHT
            - panel_height
            - 4.0)
            .max(12.0);
        let modal_card = container(option_list)
            .padding([4, 4])
            .width(panel_width)
            .style(|theme: &Theme| {
                let menu_style = style::workbench_compact_pick_list_menu_style(theme);
                container::Style {
                    background: Some(menu_style.background),
                    border: menu_style.border,
                    shadow: menu_style.shadow,
                    ..Default::default()
                }
            });
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchReasoningPicker);
        let overlay_position = iced::Point::new(panel_x.max(12.0), panel_y);

        Some(
            opaque(
                stack([
                    backdrop.into(),
                    pin(mouse_area(modal_card).on_press(Message::Noop))
                        .position(overlay_position)
                        .width(Fill)
                        .height(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }

    /// Builds the reusable blocking notice modal shown above every main-workbench surface.
    /// 构建显示在所有主工作台界面之上的可复用阻塞提示弹窗。
    pub(super) fn view_workbench_notice_modal(&self) -> Option<Element<'_, Message>> {
        let modal = self.workbench_notice_modal.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(28)
        .height(28)
        .padding(0)
        .style(style::workbench_notice_modal_close_button_style)
        .on_press(Message::CloseWorkbenchNoticeModal);
        let confirm_button = button(
            container(text(modal.confirm_label.as_str()).size(14))
                .width(Fill)
                .center(Fill),
        )
        .width(WORKBENCH_NOTICE_MODAL_ACTION_BUTTON_WIDTH)
        .padding([9, 14])
        .style(style::workbench_notice_modal_confirm_button_style)
        .on_press(Message::ConfirmWorkbenchNoticeModal);
        let mut action_row = row![space::horizontal()].spacing(10).align_y(Center);
        if let Some(dismiss_label) = modal.dismiss_label.as_deref() {
            let dismiss_button = button(
                container(text(dismiss_label).size(14))
                    .width(Fill)
                    .center(Fill),
            )
            .width(WORKBENCH_NOTICE_MODAL_ACTION_BUTTON_WIDTH)
            .padding([9, 14])
            .style(style::workbench_notice_modal_secondary_button_style)
            .on_press(Message::CloseWorkbenchNoticeModal);
            action_row = action_row.push(dismiss_button);
        }
        action_row = action_row.push(confirm_button);
        let body_scrollable = scrollable(
            container(
                text(modal.body.as_str())
                    .size(14)
                    .color(palette.background.base.text.scale_alpha(0.78))
                    .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
            )
            .width(Fill)
            .padding([2, 2]),
        )
        .width(Fill)
        .height(Fill);
        let modal_body = column![
            row![
                container(
                    codicon::warning()
                        .size(18)
                        .line_height(1.0)
                        .color(iced::color!(0xB45309)),
                )
                .width(28)
                .center_y(Fill),
                text(modal.title.as_str()).size(18),
                space::horizontal(),
                close_button,
            ]
            .spacing(10)
            .align_y(Center)
            .height(WORKBENCH_NOTICE_MODAL_HEADER_HEIGHT),
            body_scrollable,
            action_row,
        ]
        .spacing(14)
        .height(Fill);
        let modal_card = container(modal_body)
            .padding([20, 22])
            .width(WORKBENCH_NOTICE_MODAL_WIDTH)
            .height(WORKBENCH_NOTICE_MODAL_HEIGHT)
            .style(style::startup_overlay_card_style);
        let backdrop =
            mouse_area(container(space()).width(Fill).height(Fill)).on_press(Message::Noop);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }

    /// Builds the blocking image preview modal opened from one staged image attachment.
    /// 构建从某个已暂存图片附件打开的阻塞式图片预览弹窗。
    pub(super) fn view_workbench_image_preview_modal(&self) -> Option<Element<'_, Message>> {
        let modal = self.workbench_image_preview_modal.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let modal_size = self.workbench_image_preview_modal_size();
        let preview_viewport_size = self.workbench_image_preview_viewport_size();
        let zoom_percent = (modal.zoom * 100.0).round() as i32;
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_notice_modal_close_button_style)
        .on_press(Message::CloseWorkbenchImagePreviewModal);
        let zoom_out_button = button(
            container(lucide::zoom_out().size(15).line_height(1.0))
                .width(30)
                .height(30)
                .center(Fill),
        )
        .padding(0)
        .width(30)
        .height(30)
        .style(style::workbench_image_preview_control_button_style)
        .on_press(Message::AdjustWorkbenchImagePreviewZoom(
            -WORKBENCH_IMAGE_PREVIEW_BUTTON_ZOOM_STEP,
        ));
        let reset_zoom_button = button(
            container(lucide::rotate_ccw().size(15).line_height(1.0))
                .width(30)
                .height(30)
                .center(Fill),
        )
        .padding(0)
        .width(30)
        .height(30)
        .style(style::workbench_image_preview_control_button_style)
        .on_press(Message::ResetWorkbenchImagePreviewZoom);
        let zoom_in_button = button(
            container(lucide::zoom_in().size(15).line_height(1.0))
                .width(30)
                .height(30)
                .center(Fill),
        )
        .padding(0)
        .width(30)
        .height(30)
        .style(style::workbench_image_preview_control_button_style)
        .on_press(Message::AdjustWorkbenchImagePreviewZoom(
            WORKBENCH_IMAGE_PREVIEW_BUTTON_ZOOM_STEP,
        ));
        let preview_width = preview_viewport_size.width * modal.zoom;
        let preview_height = preview_viewport_size.height * modal.zoom;
        let preview_content_width = preview_width.max(preview_viewport_size.width);
        let preview_content_height = preview_height.max(preview_viewport_size.height);
        let preview_image = iced_image(modal.local_path.clone())
            .width(preview_width)
            .height(preview_height)
            .content_fit(ContentFit::Contain);
        let preview_interaction = if modal.pan_pointer.is_some() {
            iced::mouse::Interaction::Grabbing
        } else {
            iced::mouse::Interaction::Grab
        };
        let preview_scrollable = scrollable(
            container(preview_image)
                .width(preview_content_width)
                .height(preview_content_height)
                .center(Fill),
        )
        .id(workbench_image_preview_scrollable_id())
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .on_scroll(Message::WorkbenchImagePreviewScrolled)
        .width(Fill)
        .height(Fill);
        let preview_surface = mouse_area(
            container(preview_scrollable)
                .width(Fill)
                .height(Fill)
                .style(style::workbench_image_preview_surface_style),
        )
        .on_press(Message::WorkbenchImagePreviewLeftPressed)
        .on_release(Message::WorkbenchImagePreviewLeftReleased)
        .on_middle_press(Message::ResetWorkbenchImagePreviewZoom)
        .on_move(Message::WorkbenchImagePreviewPointerMoved)
        .interaction(preview_interaction);
        let zoom_percent_label = container(text(format!("{zoom_percent}%")).size(13))
            .padding([6, 10])
            .style(style::workbench_image_preview_floating_label_style);
        let zoom_toolbar = container(
            row![zoom_out_button, reset_zoom_button, zoom_in_button,]
                .spacing(6)
                .align_y(Center),
        )
        .padding(4)
        .style(style::workbench_image_preview_floating_toolbar_style);
        let preview_hint = container(
            text(self.t(I18nKey::WorkbenchImagePreviewHint))
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.72)),
        )
        .padding([6, 10])
        .style(style::workbench_image_preview_floating_label_style);
        let preview_panel = stack([
            preview_surface.into(),
            container(zoom_percent_label)
                .width(Fill)
                .height(Fill)
                .padding(12)
                .align_x(iced::Alignment::Start)
                .align_y(iced::Alignment::Start)
                .into(),
            container(zoom_toolbar)
                .width(Fill)
                .height(Fill)
                .padding(12)
                .align_x(iced::Alignment::End)
                .align_y(iced::Alignment::Start)
                .into(),
            container(preview_hint)
                .width(Fill)
                .height(Fill)
                .padding(14)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::End)
                .into(),
        ])
        .width(Fill)
        .height(Fill);
        let modal_body = column![
            row![
                container(
                    lucide::image()
                        .size(16)
                        .line_height(1.0)
                        .color(palette.primary.base.color),
                )
                .width(24)
                .center_y(Fill),
                text(self.t(I18nKey::WorkbenchImagePreviewTitle)).size(16),
                container(
                    text(modal.file_name.as_str())
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.58))
                        .wrapping(iced::advanced::text::Wrapping::None)
                )
                .width(Fill),
                close_button,
            ]
            .spacing(10)
            .align_y(Center)
            .height(WORKBENCH_IMAGE_PREVIEW_HEADER_HEIGHT),
            preview_panel,
        ]
        .spacing(WORKBENCH_IMAGE_PREVIEW_BODY_SPACING)
        .height(Fill);
        let modal_card = container(modal_body)
            .padding([
                WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_Y,
                WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_X,
            ])
            .width(modal_size.width)
            .height(modal_size.height)
            .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchImagePreviewModal);

        let overlay = stack([
            container(backdrop)
                .width(Fill)
                .height(Fill)
                .style(style::startup_overlay_backdrop_style)
                .into(),
            container(mouse_area(modal_card).on_press(Message::Noop))
                .width(Fill)
                .height(Fill)
                .center(Fill)
                .into(),
        ])
        .width(Fill)
        .height(Fill);

        Some(
            opaque(WheelIntercept::new(
                overlay,
                Message::WorkbenchImagePreviewWheelScrolled,
            ))
            .into(),
        )
    }

    /// Builds the in-app workspace-history search modal shown above the main workbench when history lookup is requested.
    /// 构建在请求历史检索时显示在主工作台之上的内嵌工作台历史搜索弹窗。
    pub(super) fn view_workspace_history_search_modal(&self) -> Option<Element<'_, Message>> {
        if !self.workspace_history_search.panel_open {
            return None;
        }

        let palette = self.app_theme.extended_palette();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_model_trigger_button_style)
        .on_press(Message::CloseWorkspaceHistorySearchPanel);

        let scope_label = self.workspace_history_search_scope_label();
        let scope_button_label = if self.workspace_history_search.project_picker_open {
            format!(
                "{}  ▲",
                self.tf(
                    I18nKey::HistorySearchScopePrefix,
                    &[I18nArg::named("scope_label", scope_label.as_str())],
                )
            )
        } else {
            format!(
                "{}  ▼",
                self.tf(
                    I18nKey::HistorySearchScopePrefix,
                    &[I18nArg::named("scope_label", scope_label.as_str())],
                )
            )
        };
        let search_input_row = row![
            text_input(
                &self.t(I18nKey::HistorySearchFilterPlaceholder),
                &self.workspace_history_search.query
            )
            .id(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
            .on_input(Message::WorkspaceHistorySearchQueryChanged)
            .on_submit(Message::SubmitWorkspaceHistorySearch)
            .padding([10, 12])
            .width(Fill),
            button(text(if self.workspace_history_search.loading {
                self.t(I18nKey::HistorySearchSearching)
            } else {
                self.t(I18nKey::HistorySearchSearchAction)
            }))
            .padding([8, 14])
            .style(style::subtle_panel_button_style)
            .on_press_maybe(
                (!self.workspace_history_search.loading
                    && !self.workspace_history_search.query.trim().is_empty())
                .then_some(Message::SubmitWorkspaceHistorySearch),
            ),
        ]
        .spacing(10)
        .align_y(Center);

        let mut modal_body = column![
            row![
                text(self.t(I18nKey::HistorySearchTitle)).size(20),
                space::horizontal(),
                close_button
            ]
            .align_y(Center),
            search_input_row,
            row![
                button(text(scope_button_label).size(12))
                    .padding([7, 12])
                    .style(style::subtle_panel_button_style)
                    .on_press(Message::ToggleWorkspaceHistorySearchProjectPicker),
                space::horizontal().width(Fill),
                text(self.t(I18nKey::HistorySearchActiveProjectsOnly)).size(11),
            ]
            .spacing(8)
            .align_y(Center),
        ]
        .spacing(14);

        if self.workspace_history_search.project_picker_open {
            let scope_options = self.filtered_workspace_history_search_scope_options();
            let scope_picker: Element<'_, Message> = if scope_options.is_empty() {
                container(
                    container(text(self.t(I18nKey::HistorySearchNoProjectMatches)).size(13))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill),
                )
                .width(Fill)
                .height(WORKSPACE_HISTORY_SEARCH_SCOPE_LIST_HEIGHT)
                .into()
            } else {
                scrollable(
                    column(scope_options.into_iter().map(|option| {
                        let is_selected = self.workspace_history_search.scope == option.scope;
                        button(
                            container(text(option.label).size(13))
                                .width(Fill)
                                .center_y(Fill),
                        )
                        .width(Fill)
                        .padding([8, 12])
                        .style(move |theme, status| {
                            style::main_task_search_result_button_style(theme, status, is_selected)
                        })
                        .on_press(Message::WorkspaceHistorySearchScopeSelected(option.scope))
                        .into()
                    }))
                    .spacing(6),
                )
                .height(WORKSPACE_HISTORY_SEARCH_SCOPE_LIST_HEIGHT)
                .into()
            };

            modal_body = modal_body.push(
                container(
                    column![
                        text_input(
                            &self.t(I18nKey::HistorySearchProjectFilterPlaceholder),
                            &self.workspace_history_search.project_filter
                        )
                        .id(WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID)
                        .on_input(Message::WorkspaceHistorySearchProjectFilterChanged)
                        .on_submit(Message::ConfirmWorkspaceHistorySearchScopeSelection)
                        .padding([10, 12]),
                        scope_picker,
                    ]
                    .spacing(10),
                )
                .padding([10, 12])
                .style(style::main_task_search_panel_style),
            );
        }

        let result_surface: Element<'_, Message> = if self.workspace_history_search.loading
            && !self.workspace_history_search.loading_more
        {
            container(
                container(text(self.t(I18nKey::HistorySearchLoading)).size(14))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKSPACE_HISTORY_SEARCH_LIST_HEIGHT)
            .into()
        } else if let Some(page) = self.workspace_history_search.results.as_ref() {
            scrollable(self.view_workspace_history_search_results(page))
                .height(WORKSPACE_HISTORY_SEARCH_LIST_HEIGHT)
                .into()
        } else if let Some(error) = self.workspace_history_search.error.as_ref() {
            container(
                container(
                    text(self.tf(
                        I18nKey::HistorySearchFailedWithError,
                        &[I18nArg::named("error", error.as_str())],
                    ))
                    .size(14),
                )
                .width(Fill)
                .height(Fill)
                .center(Fill),
            )
            .width(Fill)
            .height(WORKSPACE_HISTORY_SEARCH_LIST_HEIGHT)
            .into()
        } else {
            container(
                container(text(self.t(I18nKey::HistorySearchHint)).size(14))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKSPACE_HISTORY_SEARCH_LIST_HEIGHT)
            .into()
        };

        let modal_card = container(modal_body.push(result_surface))
            .padding([22, 24])
            .width(WORKSPACE_HISTORY_SEARCH_MODAL_WIDTH)
            .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkspaceHistorySearchPanel);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }

    /// Builds the currently selected top-level panel shown inside the main display region.
    /// 构建当前显示在主显示区域中的顶层面板内容。
    pub(super) fn view_active_main_panel(&self) -> Element<'_, Message> {
        match self.active_main_panel {
            MainDisplayPanel::Main => self.view_empty_main_panel(),
            MainDisplayPanel::PromptSkills => {
                self.view_placeholder_main_panel(MainDisplayPanel::PromptSkills)
            }
            MainDisplayPanel::LuaSkills => self.view_luaskills_panel(),
            MainDisplayPanel::Automation => {
                self.view_placeholder_main_panel(MainDisplayPanel::Automation)
            }
            MainDisplayPanel::Agents => self.view_agents_panel(),
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
    pub(super) fn view_empty_main_panel(&self) -> Element<'_, Message> {
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
    pub(super) fn view_main_activity_bar(&self) -> Element<'_, Message> {
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
    pub(super) fn view_main_activity_button(
        &self,
        panel: MainDisplayPanel,
    ) -> Element<'_, Message> {
        let tooltip_label = panel.localized_shortcut_hint(self.current_locale());
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
    pub(super) fn view_main_activity_icon(&self, panel: MainDisplayPanel) -> Element<'_, Message> {
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
    pub(super) fn view_main_task_panel(&self) -> Element<'_, Message> {
        let session_tree = scrollable(self.view_main_task_session_tree())
            .height(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(6)
                    .margin(0)
                    .scroller_width(6),
            ));

        let panel_body = column![self.view_main_task_search_panel(), session_tree]
            .spacing(10)
            .height(Fill);

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

    /// Builds the dedicated workspace-history search surface shown at the top of the left task drawer.
    /// 构建显示在左侧任务抽屉顶部的专用工作台历史搜索表面。
    fn view_main_task_search_panel(&self) -> Element<'_, Message> {
        let trigger = button(
            row![
                lucide::search().size(15).line_height(1.0),
                text("搜索").size(13),
                space::horizontal().width(Fill),
                container(text("Ctrl+F").size(11))
                    .padding([3, 7])
                    .style(style::main_task_search_shortcut_badge_style),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .width(Fill)
        .padding([8, 10])
        .style(move |theme, status| {
            style::main_task_search_result_button_style(
                theme,
                status,
                self.workspace_history_search.panel_open,
            )
        })
        .on_press(Message::OpenWorkspaceHistorySearchPanel);

        container(column![trigger].spacing(8).width(Fill))
            .width(Fill)
            .into()
    }

    /// Builds the workspace-scoped BM25 search result list shown inside the left task drawer.
    /// 构建显示在左侧任务抽屉中的工作区级 BM25 搜索结果列表。
    fn view_workspace_history_search_results(
        &self,
        page: &WorkspaceHistorySearchPage,
    ) -> Element<'_, Message> {
        if page.query.trim().is_empty() {
            return space::vertical().height(0).into();
        }
        if page.hits.is_empty() {
            return text(format!("没有找到与“{}”匹配的历史记录。", page.query))
                .size(12)
                .into();
        }

        let active_target = self
            .workspace_history_search
            .pending_navigation
            .as_ref()
            .map(|pending| (pending.session_id, pending.message_id.as_str()));
        let mut result_list = column![
            text(format!(
                "“{}” 的命中：已加载 {} / 总计 {} 条{}",
                page.query,
                page.hits.len(),
                page.total_hits,
                if page.limit_reached {
                    "（已达上限）"
                } else if page.has_more {
                    "（可继续加载）"
                } else {
                    "（已全部加载）"
                }
            ))
            .size(12)
        ]
        .spacing(6)
        .width(Fill);

        if let Some(error) = self.workspace_history_search.error.as_ref() {
            result_list = result_list.push(text(format!("最近一次搜索失败：{error}")).size(12));
        }

        for hit in &page.hits {
            let is_active = active_target.is_some_and(|(session_id, message_id)| {
                session_id == hit.session_id && message_id == hit.message_id
            });
            let kind_label = match hit.message_kind {
                SessionMessageKind::User => "USER",
                SessionMessageKind::Assistant => "AI",
                SessionMessageKind::Error => "ERROR",
            };
            let meta = format!(
                "{} · {} · 轮次 {} · {:.2}",
                hit.project_name, hit.session_title, hit.turn_index, hit.score
            );

            result_list = result_list.push(
                button(
                    container(
                        column![
                            row![
                                text(kind_label).size(11),
                                text(meta).size(11),
                                space::horizontal().width(Fill),
                                text(hit.created_at.clone()).size(11),
                            ]
                            .spacing(8)
                            .align_y(Center),
                            workspace_history_search_preview_rich_text(
                                &self.app_theme,
                                &hit.preview_markup,
                                &hit.preview,
                            ),
                        ]
                        .spacing(5)
                        .width(Fill),
                    )
                    .width(Fill),
                )
                .width(Fill)
                .padding([8, 10])
                .style(move |theme, status| {
                    style::main_task_search_result_button_style(theme, status, is_active)
                })
                .on_press(Message::OpenWorkspaceHistorySearchHit {
                    session_id: hit.session_id,
                    message_id: hit.message_id.clone(),
                    ordinal: hit.ordinal,
                    turn_index: hit.turn_index,
                }),
            );
        }

        if page.has_more || self.workspace_history_search.loading_more {
            let load_more_label = if self.workspace_history_search.loading_more {
                "正在加载更多..."
            } else {
                "加载更多"
            };
            result_list = result_list.push(
                button(text(load_more_label).size(12))
                    .padding([8, 12])
                    .style(style::subtle_panel_button_style)
                    .on_press_maybe(
                        (!self.workspace_history_search.loading_more)
                            .then_some(Message::LoadMoreWorkspaceHistorySearchResults),
                    ),
            );
        }

        result_list.into()
    }

    /// Builds the floating ghost preview that follows the cursor while one project card is being dragged.
    /// 构建在拖拽项目卡片期间跟随鼠标移动的浮动虚影预览。
    pub(super) fn view_main_task_drag_preview(&self) -> Option<Element<'_, Message>> {
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
    pub(super) fn view_main_task_panel_resize_handle(&self) -> Element<'_, Message> {
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
    pub(super) fn view_main_task_panel_restore_rail(&self) -> Element<'_, Message> {
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

    /// Builds the compact transcript surface-mode switcher shown at the trailing end of the session-status row.
    /// 构建显示在会话状态行末端的紧凑转录表面模式切换器。
    pub(super) fn view_transcript_surface_mode_switcher(&self) -> Element<'_, Message> {
        let has_selected_session = self.selected_session().is_some();
        let is_detail_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::DetailCards;
        let is_raw_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::RawSource;
        let is_request_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::ApiRequest;
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
        let request_mode_button = {
            let button =
                button(text("请求").size(12))
                    .padding([6, 10])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(
                            theme,
                            status,
                            is_request_mode,
                        )
                    });

            if has_selected_session && !is_request_mode {
                button.on_press(Message::ShowSelectedSessionDocumentApiRequest)
            } else {
                button
            }
        };

        row![detail_mode_button, raw_mode_button, request_mode_button]
            .spacing(8)
            .align_y(Center)
            .into()
    }

    /// Builds the D-shaped handle attached to the right side of the subagent dispatch panel.
    /// 构建附着在子代理调度面板右侧的 D 形把手。
    pub(super) fn view_subagent_navigation_handle(
        &self,
        is_expanded: bool,
    ) -> Element<'_, Message> {
        let handle_label = column![
            text("子").size(12),
            text("代").size(12),
            text("理").size(12),
        ]
        .spacing(1)
        .align_x(Center);

        container(handle_label)
            .width(SUBAGENT_NAVIGATION_COLLAPSED_WIDTH)
            .height(SUBAGENT_NAVIGATION_PANEL_HEIGHT)
            .center(Fill)
            .style(move |theme| style::subagent_overlay_navigation_handle_style(theme, is_expanded))
            .into()
    }

    /// Builds one rectangular row button shown inside the expanded subagent dispatch navigation panel.
    /// 构建展开的子代理调度导航面板内部显示的单个直角行按钮。
    pub(super) fn view_subagent_navigation_row_button(
        &self,
        label: String,
        on_press: Message,
        is_selected: bool,
        is_danger: bool,
    ) -> Element<'_, Message> {
        button(
            container(
                text(label)
                    .size(12)
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .width(Fill)
            .height(SUBAGENT_NAVIGATION_ROW_HEIGHT)
            .padding([0, 8])
            .clip(true)
            .align_y(Center)
            .align_x(iced::Alignment::Start),
        )
        .padding(0)
        .width(Fill)
        .height(SUBAGENT_NAVIGATION_ROW_HEIGHT)
        .style(move |theme, status| {
            style::subagent_overlay_navigation_row_button_style(
                theme,
                status,
                is_selected,
                is_danger,
            )
        })
        .on_press(on_press)
        .into()
    }

    /// Builds the compact label used by one subagent dispatch row from its agent name and optional task description.
    /// 根据代理名称与可选任务简介构建单条子代理调度行使用的紧凑标签。
    pub(super) fn subagent_navigation_task_label(&self, state: &SubagentTaskPanelState) -> String {
        let label = state.subagent_label.trim();
        let description = state
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match description {
            Some(description) if !label.contains(description) => {
                format!("{label}（{description}）")
            }
            _ if !label.is_empty() => label.to_string(),
            _ => String::from("子代理"),
        }
    }

    /// Builds the expanded subagent dispatch navigation panel rows.
    /// 构建展开状态下的子代理调度导航面板任务行。
    pub(super) fn view_expanded_subagent_navigation_panel(
        &self,
        active_subagent_task_id: Option<SessionId>,
        navigation_tasks: Vec<SubagentTaskPanelState>,
        panel_height: f32,
    ) -> Element<'_, Message> {
        let mut navigation_column = column![].spacing(SUBAGENT_NAVIGATION_ROW_GAP);
        for state in navigation_tasks {
            let is_selected = Some(state.task_id) == active_subagent_task_id;
            navigation_column = navigation_column.push(self.view_subagent_navigation_row_button(
                self.subagent_navigation_task_label(&state),
                Message::OpenSelectedSessionSubagentPanel(state.task_id),
                is_selected,
                false,
            ));
        }
        container(scrollable(navigation_column).height(Fill))
            .width(SUBAGENT_NAVIGATION_EXPANDED_WIDTH - SUBAGENT_NAVIGATION_COLLAPSED_WIDTH)
            .height(panel_height)
            .padding([8, 8])
            .style(style::subagent_overlay_navigation_panel_style)
            .into()
    }

    /// Builds one collapsible floating navigation overlay pinned against the left edge of the whole session panel instead of the inner transcript frame.
    /// 构建一个可折叠、贴着整个会话面板左边缘而非内部转录框的悬浮导航覆盖层。
    pub(super) fn view_selected_session_subagent_navigation_overlay(
        &self,
    ) -> Option<Element<'_, Message>> {
        let active_subagent_panel_state = self
            .selected_subagent_task_panel_state()
            .map(|state| self.normalized_subagent_task_panel_state(state));
        let active_subagent_task_id = active_subagent_panel_state
            .as_ref()
            .map(|state| state.task_id);
        let navigation_tasks = self.selected_session_active_subagent_tasks();

        if navigation_tasks.is_empty() {
            return None;
        }

        let is_expanded = self.subagent_navigation_expanded;
        let handle = self.view_subagent_navigation_handle(is_expanded);
        let panel_height = if is_expanded {
            subagent_navigation_panel_height(navigation_tasks.len())
        } else {
            SUBAGENT_NAVIGATION_PANEL_HEIGHT
        };
        let overlay_width = if is_expanded {
            SUBAGENT_NAVIGATION_EXPANDED_WIDTH
        } else {
            SUBAGENT_NAVIGATION_COLLAPSED_WIDTH
        };
        let overlay_content: Element<'_, Message> = if is_expanded {
            row![
                self.view_expanded_subagent_navigation_panel(
                    active_subagent_task_id,
                    navigation_tasks,
                    panel_height
                ),
                handle
            ]
            .spacing(0)
            .align_y(iced::Alignment::Start)
            .into()
        } else {
            handle
        };

        Some(
            pin(mouse_area(
                container(overlay_content)
                    .width(overlay_width)
                    .height(panel_height),
            )
            .on_enter(Message::SubagentNavigationHoverChanged(true))
            .on_exit(Message::SubagentNavigationHoverChanged(false)))
            .position(iced::Point::new(
                SUBAGENT_NAVIGATION_OVERLAY_LEFT_OFFSET,
                SUBAGENT_NAVIGATION_OVERLAY_TOP_OFFSET,
            ))
            .width(overlay_width)
            .height(SUBAGENT_NAVIGATION_OVERLAY_TOP_OFFSET + panel_height)
            .into(),
        )
    }

    /// Builds the central canvas displayed beside the task drawer in the main workbench panel.
    /// 构建显示在主工作台任务抽屉右侧的中央画布区域。
    pub(super) fn view_main_canvas(&self) -> Element<'_, Message> {
        let active_subagent_panel_state = self
            .selected_subagent_task_panel_state()
            .map(|state| self.normalized_subagent_task_panel_state(state));
        let is_subagent_panel_open = active_subagent_panel_state.is_some();
        let session_title = self
            .selected_session()
            .map(|session| session.title.as_str())
            .unwrap_or("未选择会话");
        let transcript_stream = self.view_main_canvas_transcript_stream();
        let transcript_surface_mode_switcher = self.view_transcript_surface_mode_switcher();
        let execution_mode_selector = pick_list(
            ExecutionMode::ALL,
            Some(self.execution_mode),
            Message::ExecutionModeSelected,
        )
        .width(WORKBENCH_EXECUTION_MODE_SELECTOR_WIDTH)
        .placeholder("模式")
        .padding([8, 10])
        .text_size(13)
        .line_height(iced::widget::text::LineHeight::Absolute(Pixels(20.0)))
        .style(style::workbench_compact_pick_list_style)
        .menu_style(style::workbench_compact_pick_list_menu_style);
        let composer_primary_controls = row![
            execution_mode_selector,
            self.view_workbench_model_selector(),
            self.view_workbench_openrouter_endpoint_selector(),
            self.view_workbench_ai_capability_quick_controls(),
            self.view_composer_context_usage_orb(),
            self.view_composer_attachment_button(),
        ]
        .spacing(COMPOSER_PANEL_CONTROL_GAP)
        .align_y(Center);
        let show_explicit_stop_button = self.selected_session_has_busy_execution_lane()
            && self.composer_has_submittable_content();
        let composer_secondary_controls = if show_explicit_stop_button {
            row![
                self.view_workbench_reasoning_effort_selector(),
                self.view_stop_run_button(),
                self.view_run_trigger_button(),
            ]
            .spacing(COMPOSER_PANEL_CONTROL_GAP)
            .align_y(Center)
        } else {
            row![
                self.view_workbench_reasoning_effort_selector(),
                self.view_run_trigger_button(),
            ]
            .spacing(COMPOSER_PANEL_CONTROL_GAP)
            .align_y(Center)
        };
        let composer_controls_row = row![
            composer_primary_controls,
            space::horizontal(),
            composer_secondary_controls,
        ]
        .spacing(COMPOSER_PANEL_CONTROL_GAP)
        .align_y(Center)
        .height(COMPOSER_CONTROL_ROW_HEIGHT)
        .width(Fill);
        let mut composer_editor_stack_column = column![].spacing(COMPOSER_PANEL_CONTROL_GAP);
        if let Some(hint) = self.view_composer_drag_hover_hint() {
            composer_editor_stack_column = composer_editor_stack_column.push(hint);
        }
        composer_editor_stack_column =
            composer_editor_stack_column.push(self.view_inline_composer_editor());
        if let Some(slash_panel) = self.view_composer_slash_command_panel() {
            composer_editor_stack_column = composer_editor_stack_column.push(slash_panel);
        }
        if !self.composer_attachments.is_empty() {
            composer_editor_stack_column =
                composer_editor_stack_column.push(self.view_composer_attachment_strip());
        }
        let composer_editor_stack: Element<'_, Message> = composer_editor_stack_column.into();
        let composer_body = if self.should_show_selected_session_pending_prompt_strip() {
            column![
                column![
                    self.view_pending_prompt_queue_strip(),
                    composer_editor_stack,
                ]
                .spacing(0),
                composer_controls_row,
            ]
            .spacing(COMPOSER_PANEL_CONTROL_GAP)
        } else {
            column![composer_editor_stack, composer_controls_row,]
                .spacing(COMPOSER_PANEL_CONTROL_GAP)
        };
        let composer_drag_active = !self.composer_drag_hover_labels.is_empty();
        let composer_panel = container(composer_body)
            .padding(COMPOSER_PANEL_EDGE_PADDING)
            .width(self.composer_panel_width())
            .style(move |theme| {
                style::workbench_composer_panel_drag_target_style(theme, composer_drag_active)
            });
        let title_row: Element<'_, Message> =
            if let Some(state) = active_subagent_panel_state.as_ref() {
                self.view_subagent_panel_title_row(state)
            } else {
                row![
                    text(session_title).size(26),
                    space::horizontal(),
                    transcript_surface_mode_switcher,
                ]
                .align_y(Center)
                .into()
            };
        let title_section = column![
            title_row,
            container(space())
                .width(Fill)
                .height(1)
                .style(style::main_display_bottom_separator_style),
        ]
        .spacing(12);

        let main_canvas_content = column![
            title_section,
            container(transcript_stream).width(Fill).height(Fill),
        ]
        .spacing(14)
        .height(Fill);

        let mut layered_surfaces = vec![main_canvas_content.into()];
        if !is_subagent_panel_open {
            layered_surfaces.push(
                container(
                    column![
                        space::vertical().height(Fill),
                        container(composer_panel).width(Fill).center_x(Fill),
                    ]
                    .width(Fill)
                    .height(Fill),
                )
                .width(Fill)
                .height(Fill)
                .into(),
            );
        }

        let layered_canvas: Element<'_, Message> = stack(layered_surfaces).into();

        container(layered_canvas)
            .padding([16, 16])
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Builds the reusable workspace surface shown by the example panel.
    /// 构建由示例面板承载的可复用工作区主体。
    #[allow(dead_code)]
    pub(super) fn view_workspace_surface(&self) -> Element<'_, Message> {
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
    pub(super) fn view_example_panel(&self) -> Element<'_, Message> {
        let top_row = row![
            container(self.view_example_workspace_entry_card())
                .width(FillPortion(3))
                .height(Fill),
            container(self.view_example_diff_window_launcher_card())
                .width(FillPortion(2))
                .height(Fill),
        ]
        .spacing(14)
        .height(156);

        column![
            top_row,
            container(self.view_example_editor_preview_card()).height(Fill)
        ]
        .spacing(14)
        .height(Fill)
        .into()
    }

    /// Builds the workspace-entry card dedicated to the example-panel style sandbox.
    /// 构建专供示例面板样式沙盒使用的工作区入口卡片。
    fn view_example_workspace_entry_card(&self) -> Element<'_, Message> {
        container(
            column![
                text("工作区入口").size(18),
                text_input("请输入仓库路径", &self.workspace_path)
                    .on_input(Message::WorkspacePathChanged)
                    .padding(12),
                row![
                    action_button(
                        "立即扫描",
                        "重新加载当前工作区的 Git Diff。",
                        Message::ScanWorkspace
                    ),
                    action_button(
                        "ask 调试",
                        "打开 ask 调试弹窗并在提交后回写预览结果。",
                        Message::OpenAskDebugModal
                    ),
                ]
                .spacing(10),
            ]
            .spacing(12),
        )
        .padding(16)
        .height(Fill)
        .style(container::bordered_box)
        .into()
    }

    /// Builds the patch/scaffold preview card dedicated to the example-panel ask-debug sandbox.
    /// 构建专供示例面板 ask 调试沙盒使用的补丁与骨架预览卡片。
    fn view_example_editor_preview_card(&self) -> Element<'_, Message> {
        container(
            column![
                text("补丁与骨架预览").size(18),
                scrollable(
                    text_editor(&self.example_preview_markdown_buffer)
                        .height(Shrink)
                        .padding(14)
                        .size(CODE_EDITOR_TEXT_SIZE)
                        .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                        .wrapping(iced::widget::text::Wrapping::Word)
                        .font(theme::code_font())
                        .style(style::workbench_transcript_raw_view_style)
                        .on_action(Message::ExamplePreviewMarkdownAction),
                )
                .height(Fill),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(Fill)
        .style(container::bordered_box)
        .into()
    }

    /// Builds the dedicated diff-window launcher card used by the example-panel style sandbox.
    /// 构建供示例面板样式沙盒使用的独立 Diff 窗口启动卡片。
    fn view_example_diff_window_launcher_card(&self) -> Element<'_, Message> {
        container(
            column![
                text("Git Diff").size(18),
                row![
                    action_button(
                        if self.diff_window_id.is_some() {
                            "定位窗口"
                        } else {
                            "打开窗口"
                        },
                        "打开或聚焦独立的 Git Diff 窗口。",
                        Message::OpenDiffWindow
                    ),
                    space::horizontal(),
                    text(if self.diff_window_id.is_some() {
                        "已打开"
                    } else {
                        "未打开"
                    })
                    .size(12)
                    .color(
                        self.app_theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.54)
                    ),
                ]
                .align_y(Center),
                container(text(&self.diff_summary).size(13))
                    .width(Fill)
                    .height(Fill)
                    .clip(true),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(Fill)
        .style(container::bordered_box)
        .into()
    }

    /// Builds one shared placeholder panel for not-yet-implemented top-level feature areas.
    /// 为尚未实现的顶层功能区域构建共享占位面板。
    pub(super) fn view_placeholder_main_panel(
        &self,
        panel: MainDisplayPanel,
    ) -> Element<'_, Message> {
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
                rows.extend(
                    self.runtime_foundation
                        .mcp_overview()
                        .summary_rows()
                        .iter()
                        .cloned(),
                );
                rows
            }
            MainDisplayPanel::VulcanMemoryMesh => {
                self.runtime_foundation.skill_rows(RuntimeSkillKind::Memory)
            }
            _ => Vec::new(),
        };
        let content_rows = if panel_rows.is_empty() {
            column![text(self.t(I18nKey::WorkbenchEmptyPanel)).size(14)].spacing(8)
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
                column![text(self.t(I18nKey::WorkbenchRuntimeWarnings)).size(14)].spacing(6),
                |column, warning| column.push(text(format!("• {warning}")).size(12)),
            )
        };
        container(
            column![
                text(panel.localized_section_title(self.current_locale())).size(24),
                text(panel.localized_description(self.current_locale())).size(13),
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
    pub(super) fn view_luaskills_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::LuaSkills.localized_section_title(self.current_locale()))
                    .size(22),
                text(MainDisplayPanel::LuaSkills.localized_description(self.current_locale()))
                    .size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let summary_lines = if let Some(catalog) = &self.luaskills_catalog {
            let enabled_skill_count = catalog.skills.iter().filter(|skill| skill.enabled).count();
            let managed_skill_count = catalog.skills.iter().filter(|skill| skill.managed).count();
            let hidden_tool_count = catalog
                .skills
                .iter()
                .flat_map(|skill| {
                    skill.tools.iter().filter(|tool| {
                        self.provider_configuration
                            .luaskills
                            .is_tool_disabled(&skill.skill_id, &tool.local_name)
                    })
                })
                .count();
            column![
                text(format!("运行时根：{}", catalog.runtime_root.display())).size(12),
                text(format!(
                    "system_lua_lib：{}",
                    catalog.system_lua_lib_dir.display()
                ))
                .size(12),
                text(format!(
                    "可见技能 {} · 已启用 {} · 受管安装 {} · 运行时入口 {} · 宿主隐藏工具 {}",
                    catalog.skills.len(),
                    enabled_skill_count,
                    managed_skill_count,
                    catalog.entry_count,
                    hidden_tool_count
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

        let progress_card: Option<Element<'_, Message>> =
            self.luaskills_operation_progress.as_ref().map(|progress| {
                let mut progress_body: iced::widget::Column<'_, Message> = column![
                    row![
                        text(progress.title_text()).size(14),
                        space::horizontal().width(Fill),
                        text(progress.status_line()).size(12),
                    ]
                    .align_y(Center)
                ]
                .spacing(8);
                if let Some(progress_value) = progress.progress_fraction() {
                    progress_body = progress_body.push(progress_bar(0.0..=1.0, progress_value));
                }
                if let Some(source_line) = progress.source_line() {
                    progress_body = progress_body.push(text(source_line).size(12));
                }
                if let Some(message) = progress
                    .message
                    .as_ref()
                    .map(|message| message.trim())
                    .filter(|message| !message.is_empty())
                {
                    progress_body = progress_body.push(text(message).size(12));
                }
                container(progress_body)
                    .padding(12)
                    .style(container::rounded_box)
                    .width(Fill)
                    .into()
            });

        let mut install_controls_body = column![
            row![
                text("技能目录").size(18),
                space::horizontal().width(Fill),
                refresh_button,
            ]
            .align_y(Center),
            summary_lines,
            text(&self.luaskills_notice).size(12),
        ]
        .spacing(10);
        if let Some(progress_card) = progress_card {
            install_controls_body = install_controls_body.push(progress_card);
        }
        install_controls_body = install_controls_body
            .push(
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
            )
            .push(
                text_input(
                    "可选：显式技能标识符，例如 vulcan-codekit",
                    &self.luaskills_install_skill_id,
                )
                .on_input(Message::LuaSkillsInstallSkillIdChanged)
                .padding(10),
            )
            .push(
                text_input(source_placeholder, &self.luaskills_install_source)
                    .on_input(Message::LuaSkillsInstallSourceChanged)
                    .padding(10),
            )
            .push(
                row![
                    install_button,
                    text("说明：GitHub 与包地址都走 LuaSkills 0.4.4 的受管安装链，并会回传细粒度进度。").size(12),
                ]
                .spacing(10)
                .align_y(Center),
            );

        let install_controls = container(install_controls_body)
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

    /// Builds one LuaSkills runtime tool row with host-managed visibility controls while keeping package lifecycle state unchanged.
    /// 构建带宿主管理可见性控件的单条 LuaSkills 运行时工具行，同时保持技能包生命周期状态不变。
    pub(super) fn view_luaskills_tool_row<'a>(
        &'a self,
        skill: &'a LuaSkillsCatalogSkill,
        tool: &'a crate::luaskills_runtime::LuaSkillsCatalogTool,
    ) -> Element<'a, Message> {
        let hidden = self
            .provider_configuration
            .luaskills
            .is_tool_disabled(&skill.skill_id, &tool.local_name);
        let status_text = if hidden { "已隐藏" } else { "已显示" };
        let mut toggle_button = if hidden {
            button("显示").padding([6, 10])
        } else {
            button("隐藏").padding([6, 10]).style(button::secondary)
        };
        toggle_button = toggle_button.on_press(Message::SetLuaSkillToolVisibility {
            skill_id: skill.skill_id.clone(),
            local_name: tool.local_name.clone(),
            hidden: !hidden,
        });

        let description = tool.description.trim();
        let mut body = column![
            row![
                text(&tool.local_name).size(14),
                space::horizontal().width(Fill),
                text(status_text).size(11),
                toggle_button,
            ]
            .spacing(10)
            .align_y(Center),
            text(format!("规范名：{}", tool.canonical_name)).size(12),
        ]
        .spacing(4);
        if !description.is_empty() {
            body = body.push(text(description).size(12));
        }

        container(body)
            .padding([10, 12])
            .style(container::rounded_box)
            .width(Fill)
            .into()
    }

    /// Builds the dedicated user-agent management panel backed by the dedicated user profile directory.
    /// 构建由独立用户 Agent 配置目录驱动的专用用户 Agent 管理面板。
    pub(super) fn view_agents_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::Agents.localized_section_title(self.current_locale()))
                    .size(22),
                text(MainDisplayPanel::Agents.localized_description(self.current_locale()))
                    .size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let system_default_summary = self
            .runtime_foundation
            .active_agent_definition()
            .filter(|agent| agent.id == WORKSPACE_COORDINATOR_AGENT_ID)
            .map(|agent| {
                format!(
                    "当前系统默认代理：{}（按模型自动路由基础提示词）",
                    agent.name
                )
            })
            .unwrap_or_else(|| String::from("当前系统默认代理会按模型自动路由基础提示词。"));

        let summary_card = container(
            column![
                text(&self.agent_profile_notice).size(12),
                text(format!(
                    "用户目录：{}",
                    agent_profile::agent_profiles_directory().display()
                ))
                .size(12),
                text(system_default_summary).size(12),
            ]
            .spacing(8),
        )
        .padding(16)
        .style(container::bordered_box);

        let profile_list_body = if self.agent_profile_catalog.agents.is_empty() {
            column![
                text("当前还没有用户自定义 Agent。").size(13),
                text("点击“新建 Agent”创建一份覆盖系统默认提示词的主代理。").size(12),
            ]
            .spacing(8)
        } else {
            self.agent_profile_catalog
                .agents
                .values()
                .fold(column![].spacing(8), |column, profile| {
                    column.push(self.view_user_agent_profile_card(profile))
                })
        };

        let profile_list = container(
            column![
                row![
                    column![
                        text("用户 Agent 列表").size(18),
                        text("系统默认代理不在此列表中，它会按模型自动路由。").size(12),
                    ]
                    .spacing(4),
                    space::horizontal(),
                    button("新建 Agent")
                        .padding([10, 14])
                        .style(button::secondary)
                        .on_press(Message::CreateUserAgentProfileDraft),
                ]
                .align_y(Center),
                scrollable(profile_list_body).height(Fill),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .width(FillPortion(2))
        .height(Fill);

        let selected_agent_is_current = self
            .selected_user_agent_profile_id
            .as_deref()
            .zip(
                self.agent_profile_catalog
                    .state
                    .selected_agent_id
                    .as_deref(),
            )
            .is_some_and(|(selected_id, active_id)| selected_id == active_id);
        let selected_agent_choice = self
            .selected_user_agent_profile_id
            .as_deref()
            .and_then(|agent_id| self.agent_profile_catalog.agents.get(agent_id))
            .map(|profile| AgentChoice {
                id: profile.id.clone(),
                label: profile.name.clone(),
            });

        let activation_button: Element<'_, Message> = if let Some(choice) = selected_agent_choice {
            if selected_agent_is_current {
                container(text("该 Agent 当前已激活。").size(12))
                    .padding([10, 12])
                    .style(container::rounded_box)
                    .into()
            } else {
                button("设为当前代理")
                    .padding([10, 14])
                    .style(button::secondary)
                    .on_press(Message::ActiveAgentSelected(choice))
                    .into()
            }
        } else {
            container(text("新建草稿保存后即可设为当前代理。").size(12))
                .padding([10, 12])
                .style(container::rounded_box)
                .into()
        };

        let delete_button: Element<'_, Message> = if self.selected_user_agent_profile_id.is_some() {
            button("删除")
                .padding([10, 14])
                .style(button::danger)
                .on_press(Message::DeleteUserAgentProfile)
                .into()
        } else {
            button("删除")
                .padding([10, 14])
                .style(button::danger)
                .into()
        };

        let editor_panel = container(
            scrollable(
                column![
                    row![
                        column![
                            text("编辑用户 Agent").size(18),
                            text("用户 Agent 会覆盖系统默认主提示词，并可按需追加模型与参数覆盖。")
                                .size(12),
                        ]
                        .spacing(4),
                        space::horizontal(),
                        activation_button,
                        delete_button,
                        button("保存")
                            .padding([10, 14])
                            .style(button::primary)
                            .on_press(Message::SaveUserAgentProfile),
                    ]
                    .align_y(Center),
                    column![
                        text("稳定标识").size(13),
                        text_input("留空时会根据名称自动生成", &self.agent_profile_form.id,)
                            .on_input(Message::UserAgentProfileIdChanged)
                            .padding([10, 12]),
                    ]
                    .spacing(6),
                    column![
                        text("显示名称").size(13),
                        text_input("例如：审慎代码评审代理", &self.agent_profile_form.name)
                            .on_input(Message::UserAgentProfileNameChanged)
                            .padding([10, 12]),
                    ]
                    .spacing(6),
                    column![
                        text("简短描述").size(13),
                        text_input(
                            "说明这个 Agent 更适合处理什么任务",
                            &self.agent_profile_form.description,
                        )
                        .on_input(Message::UserAgentProfileDescriptionChanged)
                        .padding([10, 12]),
                    ]
                    .spacing(6),
                    row![
                        column![
                            text("模型覆盖").size(13),
                            text_input(
                                "可选，例如 openai/gpt-5.5",
                                &self.agent_profile_form.model,
                            )
                            .on_input(Message::UserAgentProfileModelChanged)
                            .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(3)),
                        column![
                            text("温度覆盖").size(13),
                            text_input(
                                "可选，例如 0.20",
                                &self.agent_profile_form.temperature_text,
                            )
                            .on_input(Message::UserAgentProfileTemperatureChanged)
                            .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(1)),
                        column![
                            text("最大轮次").size(13),
                            text_input("可选，例如 12", &self.agent_profile_form.max_turns_text,)
                                .on_input(Message::UserAgentProfileMaxTurnsChanged)
                                .padding([10, 12]),
                        ]
                        .spacing(6)
                        .width(FillPortion(1)),
                    ]
                    .spacing(10),
                    column![
                        text("主提示词 Markdown").size(13),
                        container(
                            text_editor(&self.agent_profile_form.prompt_buffer)
                                .height(320)
                                .padding(12)
                                .size(14)
                                .wrapping(iced::widget::text::Wrapping::Word)
                                .on_action(Message::UserAgentProfilePromptAction),
                        )
                        .style(container::bordered_box),
                    ]
                    .spacing(6),
                ]
                .spacing(12),
            )
            .height(Fill),
        )
        .padding(16)
        .style(container::bordered_box)
        .width(FillPortion(3))
        .height(Fill);

        column![
            panel_header,
            summary_card,
            row![profile_list, editor_panel].spacing(12).height(Fill),
        ]
        .spacing(12)
        .height(Fill)
        .into()
    }

    /// Builds one selectable user-agent summary card shown in the left list of the Agent panel.
    /// 构建显示在 Agent 面板左侧列表中的单个可选用户 Agent 摘要卡片。
    pub(super) fn view_user_agent_profile_card<'a>(
        &self,
        profile: &'a agent_profile::UserAgentProfile,
    ) -> Element<'a, Message> {
        let is_selected =
            self.selected_user_agent_profile_id.as_deref() == Some(profile.id.as_str());
        let is_active = self
            .agent_profile_catalog
            .state
            .selected_agent_id
            .as_deref()
            == Some(profile.id.as_str());
        let active_badge: Element<'_, Message> = if is_active {
            container(text("当前激活").size(11))
                .padding([4, 8])
                .style(container::rounded_box)
                .into()
        } else {
            space::horizontal().into()
        };
        let description_text = if profile.description.trim().is_empty() {
            String::from("未填写描述")
        } else {
            profile.description.clone()
        };
        let meta_segments = [
            Some(format!("ID：{}", profile.id)),
            profile.model.as_ref().map(|model| format!("模型：{model}")),
            profile
                .temperature
                .map(|temperature| format!("温度：{temperature:.2}")),
            profile.max_turns.map(|turns| format!("最大轮次：{turns}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        button(
            column![
                row![
                    text(&profile.name).size(15),
                    space::horizontal(),
                    active_badge,
                ]
                .align_y(Center),
                text(description_text).size(12),
                text(meta_segments.join(" · ")).size(11),
            ]
            .spacing(6),
        )
        .padding([12, 14])
        .width(Fill)
        .style(if is_selected {
            button::primary
        } else {
            button::secondary
        })
        .on_press(Message::UserAgentProfileSelected(profile.id.clone()))
        .into()
    }

    /// Builds one LuaSkills skill card row used by the desktop management panel.
    /// 构建桌面管理面板使用的单个 LuaSkills 技能卡片行。
    pub(super) fn view_luaskills_skill_card<'a>(
        &'a self,
        skill: &'a LuaSkillsCatalogSkill,
    ) -> Element<'a, Message> {
        let source_text = if let Some(source_locator) = &skill.source_locator {
            let source_type = match skill.source_type {
                Some(luaskills::SkillInstallSourceType::Github) => "GitHub",
                Some(luaskills::SkillInstallSourceType::Url) => "包地址",
                Some(luaskills::SkillInstallSourceType::OfficialHub) => "Official Hub",
                Some(luaskills::SkillInstallSourceType::PrivateUrlManifest) => {
                    "Private URL Manifest"
                }
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
        let hidden_tool_count = skill
            .tools
            .iter()
            .filter(|tool| {
                self.provider_configuration
                    .luaskills
                    .is_tool_disabled(&skill.skill_id, &tool.local_name)
            })
            .count();
        let visible_tool_count = skill.tools.len().saturating_sub(hidden_tool_count);
        let active_progress = self
            .luaskills_operation_progress
            .as_ref()
            .filter(|progress| progress.matches_skill(&skill.skill_id));
        let progress_badge: Option<Element<'_, Message>> = active_progress.map(|progress| {
            container(text(progress.badge_text()).size(11))
                .padding([4, 8])
                .style(container::rounded_box)
                .into()
        });

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

        let mut detail_rows = column![
            text(format!(
                "{} · {} · 根 {} · 运行时入口 {} · 当前可见 {} · 宿主隐藏 {}",
                status_text,
                manage_text,
                skill.root_name,
                skill.entry_count,
                visible_tool_count,
                hidden_tool_count
            ))
            .size(12),
            text(format!("来源：{source_text}")).size(12),
        ]
        .spacing(4);
        if let Some(disabled_reason) = &skill.disabled_reason {
            detail_rows = detail_rows.push(text(format!("停用原因：{disabled_reason}")).size(12));
        }
        if let Some(progress) = active_progress {
            detail_rows =
                detail_rows.push(text(format!("当前进度：{}", progress.status_line())).size(12));
            if let Some(message) = progress
                .message
                .as_ref()
                .map(|message| message.trim())
                .filter(|message| !message.is_empty())
            {
                detail_rows = detail_rows.push(text(message).size(12));
            }
        }
        detail_rows =
            detail_rows.push(text(format!("路径：{}", skill.skill_dir.display())).size(12));
        let tool_section: Element<'_, Message> = if skill.tools.is_empty() {
            let empty_text = if skill.enabled {
                "当前技能尚未解析到可管理的运行时工具。"
            } else {
                "技能停用后不会加载运行时工具；重新启用后可在这里逐项管理。"
            };
            container(column![text("工具列表").size(13), text(empty_text).size(12),].spacing(6))
                .padding(12)
                .style(container::bordered_box)
                .width(Fill)
                .into()
        } else {
            let tool_rows = skill
                .tools
                .iter()
                .fold(column![].spacing(8), |column, tool| {
                    column.push(self.view_luaskills_tool_row(skill, tool))
                });
            container(
                column![
                    text("工具列表").size(13),
                    text("隐藏只影响宿主对模型暴露，不会修改技能包内容。").size(12),
                    tool_rows,
                ]
                .spacing(8),
            )
            .padding(12)
            .style(container::bordered_box)
            .width(Fill)
            .into()
        };

        container(
            column![
                {
                    let mut title_row =
                        row![text(format!("{}  v{}", skill.display_name, skill.version)).size(16)]
                            .spacing(8)
                            .align_y(Center);
                    if let Some(progress_badge) = progress_badge {
                        title_row = title_row.push(progress_badge);
                    }
                    title_row
                        .push(space::horizontal().width(Fill))
                        .push(text(format!("ID: {}", skill.skill_id)).size(12))
                },
                detail_rows,
                action_row,
                tool_section,
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
    pub(super) fn view_settings_panel(&self) -> Element<'_, Message> {
        let navigation_width = self.main_task_panel_width.clamp(260.0, 340.0);
        let mut header_title_column = column![
            text(
                self.selected_settings_section
                    .localized_title(self.current_locale())
            )
            .size(26)
        ]
        .spacing(6)
        .width(Fill);
        let header_description = self
            .selected_settings_section
            .localized_description(self.current_locale());
        if !header_description.trim().is_empty() {
            header_title_column = header_title_column.push(
                text(header_description).size(13).color(
                    self.app_theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.62),
                ),
            );
        }
        let section_header_body: Element<'_, Message> =
            if let Some(header_actions) = self.view_settings_panel_header_actions() {
                row![header_title_column, header_actions]
                    .spacing(16)
                    .width(Fill)
                    .into()
            } else {
                header_title_column.into()
            };
        let section_header = column![
            section_header_body,
            container(space())
                .width(Fill)
                .height(1)
                .style(style::main_display_bottom_separator_style),
        ]
        .spacing(6)
        .width(Fill);
        let section_body: Element<'_, Message> = match self.selected_settings_section {
            SettingsSection::ModelProviders => self.view_provider_configuration_card(),
            SettingsSection::ModelList => self.view_provider_model_list_card(),
            SettingsSection::RuntimeDiagnostics => self.view_inspector(),
            SettingsSection::PromptComposer => self.view_prompt_composer_settings_card(),
            SettingsSection::AiCapabilityVendors => self.view_ai_tool_configuration_card(),
            SettingsSection::AiCapabilityWebSearch => {
                self.view_ai_capability_settings_page(AiToolKind::WebSearch)
            }
            SettingsSection::AiCapabilityUnderstandImage => {
                self.view_ai_capability_settings_page(AiToolKind::UnderstandImage)
            }
            SettingsSection::AiCapabilityUnderstandVideo => self
                .view_ai_capability_unavailable_page(
                    "视频理解",
                    "视频理解能力入口已预留，但当前尚未接通可配置的供应商与理解工具。",
                ),
            SettingsSection::AiCapabilityUnderstandAudio => self
                .view_ai_capability_unavailable_page(
                    "音频理解",
                    "音频理解能力入口已预留，但当前尚未接通可配置的供应商与理解工具。",
                ),
            SettingsSection::AiCapabilityImageGeneration => {
                self.view_ai_capability_settings_page(AiToolKind::ImageGeneration)
            }
            SettingsSection::AiCapabilityVideoGeneration => {
                self.view_ai_capability_settings_page(AiToolKind::VideoGeneration)
            }
            SettingsSection::AiCapabilitySpeechSynthesis => {
                self.view_ai_capability_settings_page(AiToolKind::SpeechSynthesis)
            }
            SettingsSection::AiCapabilityMusicGeneration => {
                self.view_ai_capability_settings_page(AiToolKind::MusicGeneration)
            }
            SettingsSection::AiCapabilityMusicCover => {
                self.view_ai_capability_settings_page(AiToolKind::MusicCover)
            }
            SettingsSection::McpServers => self.view_mcp_configuration_card(),
            SettingsSection::AboutLicenses => self.view_about_licenses_page(),
            SettingsSection::ArchivedRestore => self.view_archived_restore_card(),
        };
        let uses_fill_layout = matches!(
            self.selected_settings_section,
            SettingsSection::ModelProviders
                | SettingsSection::ModelList
                | SettingsSection::McpServers
        ) || self.selected_settings_section.belongs_to_ai_capabilities();
        let section_body_panel: Element<'_, Message> = if uses_fill_layout {
            container(section_body).width(Fill).height(Fill).into()
        } else {
            scrollable(container(section_body).width(Fill))
                .height(Fill)
                .spacing(8)
                .into()
        };
        let content_panel: Element<'_, Message> = container(
            column![section_header, section_body_panel,]
                .spacing(14)
                .height(Fill),
        )
        .padding([16, 16])
        .width(Fill)
        .height(Fill)
        .into();

        row![
            container(self.view_settings_navigation_panel())
                .width(Pixels(navigation_width))
                .height(Fill),
            container(space())
                .width(1)
                .height(Fill)
                .style(style::main_display_left_separator_style),
            content_panel,
        ]
        .spacing(0)
        .height(Fill)
        .into()
    }

    /// Resolves the active configured provider used by the model-list settings page header and content.
    /// 解析模型列表设置页头部与内容区共用的当前已配置供应商。
    fn selected_model_list_provider_choice(&self) -> Option<ProviderChoice> {
        let configured_providers = self.configured_provider_choices();
        configured_providers
            .iter()
            .find(|provider| provider.id == self.selected_provider.id)
            .cloned()
            .or_else(|| configured_providers.first().cloned())
    }

    /// Builds optional header actions shown beside the settings-page title for sections that need top-level controls.
    /// 构建显示在设置页标题右侧、供特定分区使用的可选头部动作区。
    fn view_settings_panel_header_actions(&self) -> Option<Element<'_, Message>> {
        match self.selected_settings_section {
            SettingsSection::ModelProviders => Some(
                button("新增供应商")
                    .style(button::secondary)
                    .on_press(Message::OpenProviderEditorForCreate)
                    .into(),
            ),
            SettingsSection::ModelList => {
                let active_provider = self.selected_model_list_provider_choice()?;
                let enabled_model_count = provider_config::effective_model_choices(
                    &active_provider.id,
                    &self.provider_configuration,
                )
                .len();

                Some(
                    column![
                        button(container(text("调整模型列表")).width(Fill).center_x(Fill))
                            .width(156)
                            .style(button::primary)
                            .on_press(Message::OpenProviderModelPicker(active_provider.id)),
                        text(format!("总启用 {enabled_model_count}"))
                            .size(12)
                            .color(
                                self.app_theme
                                    .extended_palette()
                                    .background
                                    .base
                                    .text
                                    .scale_alpha(0.56),
                            ),
                    ]
                    .spacing(6)
                    .into(),
                )
            }
            SettingsSection::AiCapabilityVendors => Some(
                button("新增供应商")
                    .style(button::secondary)
                    .on_press(Message::OpenAiCapabilityVendorPicker)
                    .into(),
            ),
            SettingsSection::McpServers => Some(
                button("新增 MCP")
                    .style(button::secondary)
                    .on_press(Message::CreateMcpServer)
                    .into(),
            ),
            SettingsSection::AboutLicenses => {
                if self.about_license_catalog_loading {
                    let loading_text = text(self.about_licenses_text(
                        "正在后台加载许可证…",
                        "Loading licenses in background…",
                    ))
                    .size(12)
                    .color(
                        self.app_theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.62),
                    );
                    Some(loading_text.into())
                } else {
                    let mut actions = row![].spacing(10).align_y(Center);
                    if self
                        .about_license_catalog
                        .preferred_directory_to_open()
                        .is_some()
                    {
                        actions = actions.push(
                            button(
                                self.about_licenses_text("打开许可证目录", "Open License Folder"),
                            )
                            .style(button::secondary)
                            .on_press(Message::OpenAboutLicensesDirectory),
                        );
                    }
                    Some(actions.into())
                }
            }
            section if section.belongs_to_ai_capabilities() => {
                let capability_kind = section.capability_kind()?;
                let enabled_target_count = self
                    .ai_tool_configuration
                    .resolved_configured_targets_for_kind(capability_kind)
                    .into_iter()
                    .filter(|entry| entry.enabled)
                    .count();
                let mut actions = row![].align_y(Center).spacing(10);
                if section.supports_routing_strategy() {
                    let routing_trigger = match capability_kind {
                        AiToolKind::WebSearch => self.view_workbench_dual_line_trigger(
                            None,
                            self.ai_tool_configuration
                                .web_search
                                .mode
                                .label()
                                .to_string(),
                            String::from("路由策略"),
                            220.0,
                            true,
                            Some(Message::OpenAiCapabilityRoutingPicker(capability_kind)),
                        ),
                        AiToolKind::UnderstandImage => self.view_workbench_dual_line_trigger(
                            None,
                            self.ai_tool_configuration
                                .understand_image
                                .mode
                                .label()
                                .to_string(),
                            String::from("路由策略"),
                            220.0,
                            true,
                            Some(Message::OpenAiCapabilityRoutingPicker(capability_kind)),
                        ),
                        _ => space::horizontal().width(0).into(),
                    };
                    actions = actions.push(routing_trigger);
                }
                actions = actions.push(
                    button("配置接口")
                        .style(button::primary)
                        .on_press(Message::OpenAiCapabilityTargetPicker(capability_kind)),
                );
                Some(
                    column![
                        actions,
                        text(format!("总启用 {enabled_target_count}"))
                            .size(12)
                            .color(
                                self.app_theme
                                    .extended_palette()
                                    .background
                                    .base
                                    .text
                                    .scale_alpha(0.56),
                            ),
                    ]
                    .spacing(6)
                    .align_x(iced::Alignment::End)
                    .into(),
                )
            }
            _ => None,
        }
    }

    /// Builds the custom undecorated window title bar used by the main and diff windows.
    /// 构建主窗口与 Diff 窗口共用的自定义无系统装饰标题栏。
    pub(super) fn view_window_title_bar(
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
    pub(super) fn view_window_control_button(
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
    pub(super) fn view_window_control_icon(&self, icon: WindowControlIcon) -> Element<'_, Message> {
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
    pub(super) fn wrap_with_window_resize_handles<'a>(
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
    pub(super) fn view_window_resize_handle(
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
    pub(super) fn window_resize_interaction(
        direction: window::Direction,
    ) -> iced::mouse::Interaction {
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
    pub(super) fn is_window_maximized(&self, window_id: window::Id) -> bool {
        if self.main_window_id == Some(window_id) {
            self.main_window_maximized
        } else if self.diff_window_id == Some(window_id) {
            self.diff_window_maximized
        } else {
            false
        }
    }

    /// Returns the window title corresponding to the target window id.
    /// 根据目标窗口标识返回对应窗口标题。
    pub(super) fn title(&self, window_id: window::Id) -> String {
        if self.diff_window_id == Some(window_id) {
            String::from("Git Diff Review Panel")
        } else {
            String::from("Vulcan Code")
        }
    }

    /// Returns the current application theme to the iced runtime.
    /// 向 iced 运行时返回当前应用主题。
    pub(super) fn theme(&self, _window_id: window::Id) -> Option<Theme> {
        Some(self.app_theme.clone())
    }

    /// Subscribes to native window close events so multi-window state stays consistent.
    /// 订阅原生窗口关闭事件，以保持多窗口状态一致。
    pub(super) fn subscription(&self) -> Subscription<Message> {
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

        let transcript_follow_tail_subscription =
            if self.pending_transcript_follow_tail_session_ids.is_empty() {
                Subscription::none()
            } else {
                iced::time::every(TRANSCRIPT_STREAM_FOLLOW_TAIL_TICK_INTERVAL)
                    .map(|_| Message::SelectedSessionTranscriptFollowTailTick)
            };

        let thinking_overlay_follow_tail_subscription = if self
            .pending_thinking_overlay_follow_tail_session_ids
            .is_empty()
        {
            Subscription::none()
        } else {
            iced::time::every(TRANSCRIPT_STREAM_FOLLOW_TAIL_TICK_INTERVAL)
                .map(|_| Message::SelectedSessionThinkingOverlayFollowTailTick)
        };

        let thinking_modal_animation_subscription = if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.phase != ThinkingModalPhase::Open)
            || self
                .active_tool_modal
                .as_ref()
                .is_some_and(|modal_state| modal_state.phase != ThinkingModalPhase::Open)
        {
            iced::time::every(THINKING_MODAL_TICK_INTERVAL)
                .map(|_| Message::ThinkingModalAnimationTick)
        } else {
            Subscription::none()
        };

        let runtime_progress_animation_subscription = if self.has_runtime_progress_animation_work()
        {
            iced::time::every(SESSION_RUNTIME_PROGRESS_TICK_INTERVAL)
                .map(|_| Message::SessionRuntimeProgressTick)
        } else {
            Subscription::none()
        };

        let question_runtime_poll_subscription =
            iced::time::every(QUESTION_RUNTIME_POLL_TICK_INTERVAL)
                .map(|_| Message::QuestionRuntimePollTick);

        let luaskills_progress_poll_subscription =
            if self.luaskills_operation_in_flight && self.luaskills_runtime.is_some() {
                iced::time::every(LUASKILLS_PROGRESS_POLL_TICK_INTERVAL)
                    .map(|_| Message::PollLuaSkillsOperationProgressTick)
            } else {
                Subscription::none()
            };

        let project_rules_watch_subscription =
            project_rules_watch_subscription(&self.workspace_path);
        let prompt_resource_watch_subscription = prompt_resource_watch_subscription(
            &self.workspace_path,
            &self.provider_configuration.prompt.project_prompt_path,
            self.provider_configuration.prompt.project_prompt_enabled,
        );

        Subscription::batch([
            window::close_events().map(Message::WindowClosed),
            window::resize_events()
                .map(|(window_id, size)| Message::WindowResized(window_id, size)),
            iced::event::listen_with(Self::map_main_panel_shortcut_event),
            iced::event::listen_with(Self::map_window_pointer_event),
            transcript_drag_auto_scroll_subscription,
            thinking_overlay_drag_auto_scroll_subscription,
            transcript_follow_tail_subscription,
            thinking_overlay_follow_tail_subscription,
            thinking_modal_animation_subscription,
            runtime_progress_animation_subscription,
            question_runtime_poll_subscription,
            luaskills_progress_poll_subscription,
            project_rules_watch_subscription,
            prompt_resource_watch_subscription,
        ])
    }

    /// Maps one raw runtime event into a top-level panel switch shortcut when the key combination matches.
    /// 当按键组合匹配时，将某个原始运行时事件映射为顶层面板切换快捷键消息。
    pub(super) fn map_main_panel_shortcut_event(
        event: iced::Event,
        status: iced::event::Status,
        _window_id: window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Tab),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && modifiers.shift()
                && !modifiers.control()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::CycleExecutionMode)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                repeat,
                ..
            }) if !repeat => Some(Message::DismissTopLevelOverlay),
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowLeft),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::QuestionModalMoveToPreviousQuestion)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowRight),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::QuestionModalMoveToNextQuestion)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::WorkbenchModelPickerMoveSelectionUp)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::WorkbenchModelPickerMoveSelectionDown)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Enter),
                modifiers,
                repeat,
                ..
            }) if !repeat
                && matches!(status, iced::event::Status::Ignored)
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::ConfirmTopLevelOverlay)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) if !repeat
                && matches!(status, iced::event::Status::Ignored)
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                match key.as_ref() {
                    keyboard::Key::Named(keyboard::key::Named::Space)
                    | keyboard::Key::Character(" ") => {
                        Some(Message::QuestionModalToggleHighlightedOption)
                    }
                    _ => None,
                }
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) if !repeat && modifiers.control() && !modifiers.alt() && !modifiers.logo() => {
                match key.as_ref() {
                    keyboard::Key::Character("f") | keyboard::Key::Character("F")
                        if !modifiers.shift() =>
                    {
                        Some(Message::OpenWorkspaceHistorySearchPanel)
                    }
                    _ => None,
                }
            }
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
    pub(super) fn map_window_pointer_event(
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
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::WorkbenchKeyboardModifiersChanged(modifiers))
            }
            iced::Event::Window(window::Event::FileHovered(path)) => {
                Some(Message::WindowFileHovered(window_id, path))
            }
            iced::Event::Window(window::Event::FileDropped(path)) => {
                Some(Message::WindowFileDropped(window_id, path))
            }
            iced::Event::Window(window::Event::FilesHoveredLeft) => {
                Some(Message::WindowFilesHoverLeft(window_id))
            }
            _ => None,
        }
    }

    /// Renders the navigation pane with workspace controls and session queue.
    /// 渲染包含工作区控制与会话队列的导航面板。
    pub(super) fn view_navigation(&self) -> Element<'_, Message> {
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
    pub(super) fn view_workbench(&self) -> Element<'_, Message> {
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
                text(self.context_usage_detail.clone()).size(12),
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

    /// Renders a single session card in the navigation pane.
    /// 渲染导航面板中的单个会话卡片。
    pub(super) fn view_session_card<'a>(
        &self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
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
    pub(super) fn view_activity_card<'a>(
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
    pub(super) fn view_status_bar(&self) -> Element<'_, Message> {
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
}

#[cfg(test)]
mod tests {
    use super::{
        composer_context_anchor_reference_row, prompt_resource_watch_kind_from_file_watcher,
        shared_live_file_watcher, workspace_history_search_preview_segments,
    };
    use crate::app_shell::PromptResourceWatchKind;
    use serde_json::json;
    use std::path::PathBuf;
    use vulcan_context::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};

    /// Verifies that preview markup is split into alternating plain and highlighted segments.
    /// 验证预览标记会被拆分成按顺序交替出现的普通片段与高亮片段。
    #[test]
    fn workspace_history_search_preview_segments_split_marked_ranges() {
        let segments =
            workspace_history_search_preview_segments("开始 <mark>命中</mark> 结束", "回退文本");

        assert_eq!(
            segments,
            vec![
                (String::from("开始 "), false),
                (String::from("命中"), true),
                (String::from(" 结束"), false),
            ]
        );
    }

    /// Verifies that malformed preview markup still preserves the highlighted tail instead of exposing raw tags.
    /// 验证格式不完整的预览标记仍会保留高亮尾段，而不会把原始标签直接暴露出来。
    #[test]
    fn workspace_history_search_preview_segments_tolerate_unclosed_mark() {
        let segments = workspace_history_search_preview_segments("前缀 <mark>高亮尾段", "回退文本");

        assert_eq!(
            segments,
            vec![
                (String::from("前缀 "), false),
                (String::from("高亮尾段"), true),
            ]
        );
    }

    /// Verifies prompt-resource watch paths are classified before cache invalidation.
    /// 验证提示词资源监听路径会在缓存失效前完成分类。
    #[test]
    fn prompt_resource_watch_kind_classifies_specific_paths() {
        let target =
            vulcan_file_watcher::adapters::prompt_resources::PromptResourcesWatchTarget::new(
                PathBuf::from("D:/repo"),
                ".vulcan/code/prompt.md",
                true,
                Some(PathBuf::from("C:/Users/me/.vulcan/code/prompt")),
            );

        assert_eq!(
            prompt_resource_watch_kind_from_file_watcher(
                target.classify_paths(&[PathBuf::from("D:/repo/.vulcan/code/prompt.md")])
            ),
            PromptResourceWatchKind::ProjectPrompt
        );
        assert_eq!(
            prompt_resource_watch_kind_from_file_watcher(
                target.classify_paths(&[PathBuf::from("D:/repo/.vulcan/code/config.json")])
            ),
            PromptResourceWatchKind::ProjectPromptConfig
        );
        assert_eq!(
            prompt_resource_watch_kind_from_file_watcher(target.classify_paths(&[PathBuf::from(
                "C:/Users/me/.vulcan/code/prompt/personalities/a.md"
            )])),
            PromptResourceWatchKind::UserPromptResources
        );
    }

    /// Verifies GUI filesystem subscriptions reuse one shared live watcher backend.
    /// 验证 GUI 文件系统订阅会复用同一个共享实时监听后端。
    #[test]
    fn shared_live_file_watcher_reuses_process_instance() {
        let first = shared_live_file_watcher();
        let second = shared_live_file_watcher();

        match (first, second) {
            (Some(first), Some(second)) => assert!(std::sync::Arc::ptr_eq(&first, &second)),
            (None, None) => {}
            _ => panic!("shared watcher initialization should be stable"),
        }
    }

    /// Verifies that compaction-anchor rows stay short and only expose the linked summary message identity and headline.
    /// 验证压缩锚点行保持短小，仅暴露关联摘要消息的身份与标题。
    #[test]
    fn composer_context_anchor_reference_row_uses_short_summary_identity() {
        let message = SessionMessageEntry {
            id: String::from("summary-anchor-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::CompactionSummary,
            visible: false,
            title: String::from("Context Continuation Prompt"),
            body: String::from(
                "This session is being continued from earlier context that was compacted to fit the model context window.\n\nEarlier context summary:\n保留了目标、工具、未完成项和基线状态。\n\nContinue from the newer messages below without restarting the conversation, repeating this summary, or asking for the same information again.",
            ),
            metadata: None,
            created_at: String::from("20"),
        };

        let row = composer_context_anchor_reference_row("summary-anchor-1", &message);

        assert_eq!(
            row,
            "压缩锚点：已建立 · 保留了目标、工具、未完成项和基线状态。"
        );
    }

    /// Verifies compaction anchor rows prefer metadata previews over parsing the synthetic continuation prompt body.
    /// 验证压缩锚点行会优先使用元数据预览，而不是解析合成续写提示词正文。
    #[test]
    fn composer_context_anchor_reference_row_prefers_metadata_preview() {
        let message = SessionMessageEntry {
            id: String::from("summary-anchor-2"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::CompactionSummary,
            visible: false,
            title: String::from("Context Continuation Prompt"),
            body: String::from("This body should not be parsed when preview metadata exists."),
            metadata: Some(json!({
                "preview": "保留目标与未完成工作",
            })),
            created_at: String::from("21"),
        };

        let row = composer_context_anchor_reference_row("summary-anchor-2", &message);

        assert_eq!(row, "压缩锚点：已建立 · 保留目标与未完成工作");
    }
}

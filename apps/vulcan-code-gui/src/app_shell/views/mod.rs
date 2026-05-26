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
mod diff;
mod settings_navigation;
mod task_tree;
mod transcript;

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

    /// Builds the blocking ask/question modal shown while one structured follow-up tool call is waiting for the user.
    /// 构建在某条结构化跟进提问工具调用等待用户时显示的阻塞式 ask/question 弹窗。
    pub(super) fn view_workbench_question_modal(&self) -> Option<Element<'_, Message>> {
        let modal = self.active_question_modal.as_ref()?;
        if modal.request.questions.is_empty() {
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
        .width(28)
        .height(28)
        .padding(0)
        .style(style::workbench_notice_modal_close_button_style)
        .on_press(Message::DismissTopLevelOverlay);
        let question_count = modal.request.questions.len();
        let active_question_index = modal
            .active_question_index
            .min(question_count.saturating_sub(1));
        let unanswered_questions = self.collect_active_question_modal_unanswered_questions();
        let all_answered = unanswered_questions.is_empty();
        let modal_subtitle = match modal.stage {
            WorkbenchQuestionModalStage::Answering => {
                format!("第 {}/{} 题", active_question_index + 1, question_count)
            }
            WorkbenchQuestionModalStage::Confirming => String::from("确认"),
        };
        let question_tabs = row(modal
            .request
            .questions
            .iter()
            .enumerate()
            .map(|(question_index, question)| {
                let draft = modal
                    .drafts
                    .get(question_index)
                    .cloned()
                    .unwrap_or_default();
                let is_answered =
                    Self::collect_question_modal_prompt_answers(question, &draft).is_ok();
                let is_active = modal.stage == WorkbenchQuestionModalStage::Answering
                    && question_index == active_question_index;
                button(
                    container(
                        row![
                            text(if is_answered { "●" } else { "○" }).size(10).color(
                                if is_answered {
                                    palette.success.strong.color
                                } else {
                                    palette.background.base.text.scale_alpha(0.38)
                                }
                            ),
                            container(text(question.header.as_str()).size(13))
                                .width(Fill)
                                .clip(true),
                        ]
                        .spacing(8)
                        .align_y(Center)
                        .width(Fill),
                    )
                    .width(Fill)
                    .center_y(Fill),
                )
                .width(120)
                .height(36)
                .padding([0, 10])
                .style(move |theme, status| {
                    style::workbench_model_picker_option_button_style(theme, status, is_active)
                })
                .on_press(Message::QuestionModalSelectQuestionTab(question_index))
                .into()
            })
            .chain(std::iter::once({
                let is_active = modal.stage == WorkbenchQuestionModalStage::Confirming;
                button(
                    container(
                        row![
                            text(if all_answered { "●" } else { "○" }).size(10).color(
                                if all_answered {
                                    palette.success.strong.color
                                } else {
                                    palette.background.base.text.scale_alpha(0.38)
                                }
                            ),
                            container(text("确认").size(13)).width(Fill).clip(true),
                        ]
                        .spacing(8)
                        .align_y(Center)
                        .width(Fill),
                    )
                    .width(Fill)
                    .center_y(Fill),
                )
                .width(120)
                .height(36)
                .padding([0, 10])
                .style(move |theme, status| {
                    style::workbench_model_picker_option_button_style(theme, status, is_active)
                })
                .on_press(Message::QuestionModalSelectConfirmTab)
                .into()
            })))
        .spacing(8)
        .align_y(Center);
        let question_tabs_surface: Element<'_, Message> =
            scrollable(container(question_tabs).width(Shrink))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default(),
                ))
                .height(42)
                .width(Fill)
                .into();
        let body_surface: Element<'_, Message> = match modal.stage {
            WorkbenchQuestionModalStage::Answering => {
                let current_question = modal.request.questions.get(active_question_index)?;
                let current_draft = modal
                    .drafts
                    .get(active_question_index)
                    .cloned()
                    .unwrap_or_default();
                let selected_single_option_index =
                    current_question.options.iter().position(|option| {
                        current_draft
                            .selected_labels
                            .iter()
                            .any(|selected_label| selected_label == &option.label)
                    });
                let option_rows: Element<'_, Message> = if current_question.options.is_empty() {
                    text_input("请输入回答", &current_draft.custom_text)
                        .id(WORKBENCH_QUESTION_MODAL_CUSTOM_INPUT_ID)
                        .on_input(move |value| Message::QuestionModalCustomTextChanged {
                            question_index: active_question_index,
                            value,
                        })
                        .on_submit(Message::ConfirmTopLevelOverlay)
                        .padding([10, 12])
                        .into()
                } else {
                    let custom_option_index =
                        Self::question_modal_custom_option_index(current_question);
                    let option_list = current_question.options.iter().enumerate().map(
                        |(option_index, option)| {
                            let is_selected = current_draft
                                .selected_labels
                                .iter()
                                .any(|selected_label| selected_label == &option.label);
                            let is_highlighted = option_index == modal.active_option_index;
                            let has_description = !option.description.trim().is_empty();
                            let row_message = Message::QuestionModalOptionPressed {
                                question_index: active_question_index,
                                option_index,
                            };
                            let control_message = row_message.clone();
                            let control: Element<'_, Message> = if current_question
                                .allows_multiple()
                            {
                                checkbox(is_selected)
                                    .size(16)
                                    .spacing(0)
                                    .on_toggle({
                                        let control_message = control_message.clone();
                                        move |_| control_message.clone()
                                    })
                                    .into()
                            } else {
                                radio("", option_index, selected_single_option_index, move |_| {
                                    control_message.clone()
                                })
                                .size(16)
                                .spacing(0)
                                .into()
                            };
                            let indicator: Element<'_, Message> = if is_highlighted {
                                codicon::chevron_right()
                                    .size(14)
                                    .line_height(1.0)
                                    .color(palette.primary.base.color)
                                    .into()
                            } else {
                                text("").size(14).into()
                            };
                            let description_surface: Element<'_, Message> = if has_description {
                                container(
                                    text(option.description.as_str())
                                        .size(12)
                                        .color(palette.background.base.text.scale_alpha(0.54))
                                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                                )
                                .padding([0, 34])
                                .width(Fill)
                                .into()
                            } else {
                                space::vertical().height(0).into()
                            };

                            mouse_area(
                                container(
                                    column![
                                        row![
                                            container(indicator).width(18).center_y(Fill),
                                            container(control).width(16).center_y(Fill),
                                            container(text(option.label.as_str()).size(15))
                                                .width(Fill)
                                                .clip(true),
                                        ]
                                        .spacing(10)
                                        .align_y(Center),
                                        description_surface,
                                    ]
                                    .spacing(if has_description { 4 } else { 0 }),
                                )
                                .padding([7, 10])
                                .width(Fill)
                                .style(move |theme: &Theme| {
                                    let mut row_style =
                                        style::workbench_ai_capability_route_row_style(
                                            theme, false,
                                        );
                                    let palette = theme.extended_palette();
                                    row_style.background =
                                        Some(Background::Color(if is_highlighted {
                                            palette.primary.weak.color.scale_alpha(0.18)
                                        } else if is_selected {
                                            palette.primary.weak.color.scale_alpha(0.10)
                                        } else {
                                            palette.background.base.color.scale_alpha(0.72)
                                        }));
                                    row_style.border.color = if is_highlighted {
                                        palette.primary.base.color.scale_alpha(0.58)
                                    } else if is_selected {
                                        palette.primary.base.color.scale_alpha(0.26)
                                    } else {
                                        palette.background.strong.color.scale_alpha(0.20)
                                    };
                                    row_style
                                }),
                            )
                            .on_press(row_message)
                            .into()
                        },
                    );
                    let custom_row = {
                        let is_selected = current_draft.custom_selected;
                        let is_highlighted = custom_option_index == modal.active_option_index;
                        let row_message = Message::QuestionModalOptionPressed {
                            question_index: active_question_index,
                            option_index: custom_option_index,
                        };
                        let control_message = row_message.clone();
                        let control: Element<'_, Message> = if current_question.allows_multiple() {
                            checkbox(is_selected)
                                .size(16)
                                .spacing(0)
                                .on_toggle({
                                    let control_message = control_message.clone();
                                    move |_| control_message.clone()
                                })
                                .into()
                        } else {
                            radio(
                                "",
                                custom_option_index,
                                current_draft.custom_selected.then_some(custom_option_index),
                                move |_| control_message.clone(),
                            )
                            .size(16)
                            .spacing(0)
                            .into()
                        };
                        let indicator: Element<'_, Message> = if is_highlighted {
                            codicon::chevron_right()
                                .size(14)
                                .line_height(1.0)
                                .color(palette.primary.base.color)
                                .into()
                        } else {
                            text("").size(14).into()
                        };

                        mouse_area(
                            container(
                                row![
                                    container(indicator).width(18).center_y(Fill),
                                    container(control).width(16).center_y(Fill),
                                    container(text("其他回答").size(15)).width(Fill).clip(true),
                                ]
                                .spacing(10)
                                .align_y(Center),
                            )
                            .padding([7, 10])
                            .width(Fill)
                            .style(move |theme: &Theme| {
                                let mut row_style =
                                    style::workbench_ai_capability_route_row_style(theme, false);
                                let palette = theme.extended_palette();
                                row_style.background = Some(Background::Color(if is_highlighted {
                                    palette.primary.weak.color.scale_alpha(0.18)
                                } else if is_selected {
                                    palette.primary.weak.color.scale_alpha(0.10)
                                } else {
                                    palette.background.base.color.scale_alpha(0.72)
                                }));
                                row_style.border.color = if is_highlighted {
                                    palette.primary.base.color.scale_alpha(0.58)
                                } else if is_selected {
                                    palette.primary.base.color.scale_alpha(0.26)
                                } else {
                                    palette.background.strong.color.scale_alpha(0.20)
                                };
                                row_style
                            }),
                        )
                        .on_press(row_message)
                        .into()
                    };
                    let custom_input_surface: Element<'_, Message> =
                        if current_draft.custom_selected {
                            text_input("请输入自定义回答", &current_draft.custom_text)
                                .id(WORKBENCH_QUESTION_MODAL_CUSTOM_INPUT_ID)
                                .on_input(move |value| Message::QuestionModalCustomTextChanged {
                                    question_index: active_question_index,
                                    value,
                                })
                                .on_submit(Message::ConfirmTopLevelOverlay)
                                .padding([10, 12])
                                .into()
                        } else {
                            space::vertical().height(0).into()
                        };
                    column![
                        scrollable(
                            column(option_list.chain(std::iter::once(custom_row))).spacing(8)
                        )
                        .width(Fill)
                        .height(Fill),
                        custom_input_surface,
                    ]
                    .spacing(12)
                    .height(Fill)
                    .into()
                };
                let hint_text = if current_question.options.is_empty() {
                    "Enter 下一页 · Esc 取消"
                } else if current_question.allows_multiple() {
                    "↑↓ 选择 · 空格勾选 · Enter 下一页 · ←→ 切换标签 · Esc 取消"
                } else {
                    "↑↓ 选择 · 空格勾选 · Enter 下一页 · ←→ 切换标签 · Esc 取消"
                };

                column![
                    row![
                        text(if current_question.options.is_empty() {
                            "输入"
                        } else if current_question.allows_multiple() {
                            "多选"
                        } else {
                            "单选"
                        })
                        .size(12)
                        .color(palette.primary.base.color),
                        space::horizontal(),
                        text(hint_text)
                            .size(12)
                            .color(palette.background.base.text.scale_alpha(0.48)),
                    ]
                    .align_y(Center),
                    text(current_question.question.as_str())
                        .size(18)
                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                    option_rows,
                ]
                .spacing(12)
                .height(Fill)
                .into()
            }
            WorkbenchQuestionModalStage::Confirming => {
                let summary_cards = column(modal.request.questions.iter().enumerate().map(
                    |(question_index, question)| {
                        let draft = modal
                            .drafts
                            .get(question_index)
                            .cloned()
                            .unwrap_or_default();
                        let answers = Self::collect_question_modal_prompt_answers(question, &draft)
                            .unwrap_or_else(|_| vec![String::from("未填写")]);
                        container(
                            row![
                                column![
                                    text(question.header.as_str())
                                        .size(12)
                                        .color(palette.primary.base.color),
                                    text(question.question.as_str())
                                        .size(14)
                                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                                    text(answers.join(" / "))
                                        .size(13)
                                        .color(palette.background.base.text.scale_alpha(0.74))
                                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                                ]
                                .spacing(6)
                                .width(Fill),
                                button(text("修改").size(12))
                                    .padding([4, 8])
                                    .style(style::workbench_notice_modal_secondary_button_style)
                                    .on_press(Message::QuestionModalSelectQuestionTab(
                                        question_index
                                    )),
                            ]
                            .spacing(12)
                            .align_y(Center),
                        )
                        .padding([12, 14])
                        .width(Fill)
                        .style(|theme: &Theme| {
                            let palette = theme.extended_palette();
                            iced::widget::container::Style {
                                background: Some(Background::Color(
                                    palette.background.weak.color.scale_alpha(0.24),
                                )),
                                border: Border {
                                    radius: 0.0.into(),
                                    width: 1.0,
                                    color: palette.background.strong.color.scale_alpha(0.16),
                                },
                                ..Default::default()
                            }
                        })
                        .into()
                    },
                ))
                .spacing(12);
                let missing_surface: Element<'_, Message> = if all_answered {
                    container(
                        text("已回答全部问题，确认提交后会继续执行。")
                            .size(13)
                            .color(palette.background.base.text.scale_alpha(0.62)),
                    )
                    .width(Fill)
                    .into()
                } else {
                    container(
                        column(
                            std::iter::once(
                                text("没有回答完全部问题，请先完成以下问题。")
                                    .size(13)
                                    .color(iced::color!(0xB91C1C))
                                    .into(),
                            )
                            .chain(
                                unanswered_questions.clone().into_iter().map(
                                    |(question_index, header, error)| {
                                        button(
                                            container(
                                                column![
                                                    text(header).size(13),
                                                    text(error).size(12).color(
                                                        palette
                                                            .background
                                                            .base
                                                            .text
                                                            .scale_alpha(0.54)
                                                    ),
                                                ]
                                                .spacing(4)
                                                .width(Fill),
                                            )
                                            .width(Fill),
                                        )
                                        .width(Fill)
                                        .padding([8, 10])
                                        .style(style::workbench_notice_modal_secondary_button_style)
                                        .on_press(Message::QuestionModalSelectQuestionTab(
                                            question_index,
                                        ))
                                        .into()
                                    },
                                ),
                            ),
                        )
                        .spacing(8),
                    )
                    .width(Fill)
                    .into()
                };
                let action_row: Element<'_, Message> = if all_answered {
                    row![
                        space::horizontal(),
                        container(
                            mouse_area(
                                container(codicon::check().size(20).line_height(1.0))
                                    .width(Fill)
                                    .height(Fill)
                                    .center(Fill)
                                    .style(
                                        style::workbench_question_modal_submit_icon_surface_style
                                    )
                            )
                            .on_press(Message::SubmitQuestionModal)
                        )
                        .width(52)
                        .height(52),
                        space::horizontal(),
                    ]
                    .align_y(Center)
                    .into()
                } else {
                    space::vertical().height(0).into()
                };

                column![
                    missing_surface,
                    scrollable(summary_cards).width(Fill).height(Fill),
                    action_row
                ]
                .spacing(12)
                .height(Fill)
                .into()
            }
        };
        let validation_surface: Element<'_, Message> =
            if let Some(error) = modal.validation_error.as_deref() {
                container(
                    text(error)
                        .size(13)
                        .color(iced::color!(0xB91C1C))
                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                )
                .width(Fill)
                .into()
            } else {
                space::vertical().height(0).into()
            };
        let modal_body = column![
            row![
                text("回答问题").size(18),
                container(
                    text(modal_subtitle)
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.52)),
                )
                .padding([4, 8]),
                space::horizontal(),
                close_button,
            ]
            .spacing(10)
            .align_y(Center)
            .height(WORKBENCH_QUESTION_MODAL_HEADER_HEIGHT),
            question_tabs_surface,
            body_surface,
            validation_surface,
        ]
        .spacing(12)
        .height(Fill);
        let modal_card = container(modal_body)
            .padding([22, 24])
            .width(WORKBENCH_QUESTION_MODAL_WIDTH)
            .height(WORKBENCH_QUESTION_MODAL_HEIGHT)
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

    /// Builds the project settings modal opened from the task-tree project context menu.
    /// 构建从任务树项目右键菜单打开的项目设置弹窗。
    pub(super) fn view_task_project_settings_modal(&self) -> Option<Element<'_, Message>> {
        let project = self.project_settings_task_project()?;
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
        .on_press(Message::CloseTaskProjectSettings);
        let attachment_root =
            crate::attachment_assets::resolve_managed_attachment_root(&project.path);
        let project_summary = container(
            column![
                text("项目空间").size(14),
                text(format!("项目路径：{}", project.path)).size(12),
                text(format!("附件缓存：{}", attachment_root.display())).size(12),
            ]
            .spacing(6),
        )
        .padding(12)
        .style(container::bordered_box);
        let modal_card = container(
            column![
                row![
                    column![
                        text(format!("项目设置：{}", project.title)).size(20),
                        text("项目级配置与缓存清理只作用于当前项目空间。").size(12),
                    ]
                    .spacing(4)
                    .width(Fill),
                    close_button,
                ]
                .align_y(Center),
                project_summary,
                self.view_project_attachment_cleanup_card(project),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(TASK_PROJECT_SETTINGS_MODAL_WIDTH)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseTaskProjectSettings);

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

    /// Returns whether the current composer buffer contains any submittable user instruction text.
    /// 返回当前提示词缓冲区是否包含任何可提交的用户指令文本。
    pub(super) fn composer_has_submittable_content(&self) -> bool {
        composer_input_has_submittable_content(
            &self.composer_buffer.text(),
            &self.composer_attachments,
        )
    }

    /// Builds the compact send-or-stop button shown at the right side of prompt inputs.
    /// 构建显示在提示词输入框右侧的紧凑发送或停止按钮。
    pub(super) fn view_run_trigger_button(&self) -> Element<'_, Message> {
        let is_busy = self.selected_session_has_busy_execution_lane();
        let can_submit = self.composer_has_submittable_content();
        let show_stop = is_busy && !can_submit;
        let tooltip_text = if show_stop {
            "停止全部 AI 调用与等待中的系统任务"
        } else if is_busy {
            if can_submit {
                "将当前任务指令加入待插入列表"
            } else {
                "停止全部 AI 调用与等待中的系统任务"
            }
        } else if can_submit {
            "发送当前任务指令"
        } else {
            "请输入任务指令后再发送"
        };
        let icon: Element<'_, Message> = if show_stop {
            codicon::debug_stop()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0)
                .into()
        } else {
            lucide::send()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0)
                .into()
        };
        let icon_surface: Element<'_, Message> = container(icon)
            .width(COMPOSER_TRIGGER_BUTTON_SIZE)
            .height(COMPOSER_TRIGGER_BUTTON_SIZE)
            .center(Fill)
            .into();

        let button = button(icon_surface).padding(0).style(move |theme, status| {
            style::workbench_composer_trigger_button_style(theme, status, show_stop)
        });
        let button = if show_stop {
            button.on_press(Message::StopRun)
        } else if can_submit {
            button.on_press(Message::StartRun)
        } else {
            button
        };

        tooltip(
            button
                .width(COMPOSER_TRIGGER_BUTTON_SIZE)
                .height(COMPOSER_TRIGGER_BUTTON_SIZE),
            tooltip_text,
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Builds the dedicated stop button kept beside the queue/send trigger while one session still owns any busy execution lane.
    /// 构建在某个会话仍占有繁忙执行通道时保留在排队发送按钮旁边的独立停止按钮。
    pub(super) fn view_stop_run_button(&self) -> Element<'_, Message> {
        let is_busy = self.selected_session_has_busy_execution_lane();
        let icon_surface: Element<'_, Message> = container(
            codicon::debug_stop()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0),
        )
        .width(COMPOSER_TRIGGER_BUTTON_SIZE)
        .height(COMPOSER_TRIGGER_BUTTON_SIZE)
        .center(Fill)
        .into();
        let button = button(icon_surface).padding(0).style(move |theme, status| {
            style::workbench_composer_trigger_button_style(theme, status, true)
        });
        let button = if is_busy {
            button.on_press(Message::StopRun)
        } else {
            button
        };

        tooltip(
            button
                .width(COMPOSER_TRIGGER_BUTTON_SIZE)
                .height(COMPOSER_TRIGGER_BUTTON_SIZE),
            "停止全部 AI 调用与等待中的系统任务",
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Builds the success-colored circular return button shown before a subagent panel title.
    /// 构建显示在子代理面板标题左侧的成功色圆形返回按钮。
    pub(super) fn view_subagent_panel_return_button(&self) -> Element<'_, Message> {
        let icon_surface: Element<'_, Message> =
            container(codicon::chevron_left().size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE))
                .width(COMPOSER_TRIGGER_BUTTON_SIZE)
                .height(COMPOSER_TRIGGER_BUTTON_SIZE)
                .center(Fill)
                .into();
        let button = button(icon_surface)
            .padding(0)
            .width(COMPOSER_TRIGGER_BUTTON_SIZE)
            .height(COMPOSER_TRIGGER_BUTTON_SIZE)
            .style(style::subagent_panel_return_button_style)
            .on_press(Message::ReturnToSelectedSessionTranscript);

        tooltip(button, "返回主会话", tooltip::Position::Bottom)
            .style(container::rounded_box)
            .into()
    }

    /// Builds the dedicated stop button shown at the right side of a running subagent panel title.
    /// 构建显示在运行中子代理面板标题右侧的专用停止按钮。
    pub(super) fn view_subagent_panel_stop_button(&self) -> Element<'_, Message> {
        let icon_surface: Element<'_, Message> = container(
            codicon::debug_stop()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0),
        )
        .width(COMPOSER_TRIGGER_BUTTON_SIZE)
        .height(COMPOSER_TRIGGER_BUTTON_SIZE)
        .center(Fill)
        .into();
        let button = button(icon_surface)
            .padding(0)
            .width(COMPOSER_TRIGGER_BUTTON_SIZE)
            .height(COMPOSER_TRIGGER_BUTTON_SIZE)
            .style(move |theme, status| {
                style::workbench_composer_trigger_button_style(theme, status, true)
            })
            .on_press(Message::StopSelectedSubagentTask);

        tooltip(button, "停止当前子代理", tooltip::Position::Bottom)
            .style(container::rounded_box)
            .into()
    }

    /// Builds the title row for the dedicated subagent conversation surface.
    /// 构建独立子代理会话表面的标题行。
    pub(super) fn view_subagent_panel_title_row(
        &self,
        state: &SubagentTaskPanelState,
    ) -> Element<'_, Message> {
        let mut title_row = row![
            self.view_subagent_panel_return_button(),
            text(state.subagent_label.clone()).size(26),
            space::horizontal(),
        ]
        .spacing(10)
        .align_y(Center);
        if state.run_state.is_running() {
            title_row = title_row.push(self.view_subagent_panel_stop_button());
        }

        title_row.into()
    }

    /// Builds the insertion indicator shown before one pending-prompt queue row while reordering.
    /// 构建待发送队列重排期间显示在某条队列行之前的插入指示器。
    pub(super) fn view_pending_prompt_queue_drop_indicator(
        &self,
        is_visible: bool,
    ) -> Element<'_, Message> {
        let height = if is_visible {
            COMPOSER_PENDING_QUEUE_DROP_INDICATOR_HEIGHT
        } else {
            0.0
        };
        let indicator = if is_visible {
            container(space())
                .width(Fill)
                .height(2.0)
                .style(style::workbench_pending_prompt_drop_indicator_style)
        } else {
            container(space()).width(Fill).height(0.0)
        };
        container(indicator)
            .padding(iced::Padding {
                top: 0.0,
                right: 2.0,
                bottom: if is_visible { 2.0 } else { 0.0 },
                left: 2.0,
            })
            .height(height)
            .width(Fill)
            .into()
    }

    /// Builds one draggable queue row rendered inside the pending-prompt queue panel.
    /// 构建渲染在待发送队列面板中的单条可拖拽队列行。
    pub(super) fn view_pending_prompt_queue_row(
        &self,
        index: usize,
        entry: PendingPromptPanelEntry,
    ) -> Element<'_, Message> {
        let is_drag_source =
            self.dragging_pending_prompt_entry_id.as_deref() == Some(entry.id.as_str());
        let queue_label = pending_prompt_panel_label_text(&entry.prompt, &entry.attachments);
        let drag_handle = mouse_area(
            container(
                text("⋮⋮")
                    .size(13)
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .center_x(14)
            .center_y(Fill),
        )
        .interaction(if self.dragging_pending_prompt_entry_id.is_some() {
            iced::mouse::Interaction::Move
        } else {
            iced::mouse::Interaction::Grab
        })
        .on_press(Message::StartPendingPromptQueueDrag(entry.id.clone()))
        .on_move({
            let entry_id = entry.id.clone();
            move |position| Message::PendingPromptQueueDropPreviewMoved(entry_id.clone(), position)
        });
        let label_surface: Element<'_, Message> = scrollable(
            container(
                text(queue_label)
                    .size(13)
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .width(Shrink),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .height(COMPOSER_PENDING_QUEUE_ROW_HEIGHT)
        .width(Fill)
        .into();
        let action_button = |label: &'static str, message: Option<Message>| {
            let button =
                button(text(label).size(12))
                    .padding([4, 8])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(theme, status, false)
                    });
            if let Some(message) = message {
                button.on_press(message)
            } else {
                button
            }
        };
        let action_surface: Element<'_, Message> =
            if entry.state == PendingPromptPanelEntryState::Inserted {
                container(
                    text("已接入系统队列").size(12).color(
                        self.app_theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.56),
                    ),
                )
                .padding([0, 6])
                .center_y(Fill)
                .into()
            } else {
                row![
                    action_button(
                        "插入",
                        Some(Message::PendingPromptQueueInsertNow(entry.id.clone()))
                    ),
                    action_button(
                        "编辑",
                        Some(Message::PendingPromptQueueEdit(entry.id.clone()))
                    ),
                    action_button(
                        "移除",
                        Some(Message::PendingPromptQueueRemove(entry.id.clone()))
                    ),
                ]
                .spacing(6)
                .align_y(Center)
                .into()
            };
        container(
            row![
                drag_handle,
                container(text(format!("{:02}", index + 1)).size(12))
                    .width(24)
                    .center_y(Fill),
                container(text("·").size(12)).width(10).center_y(Fill),
                container(label_surface).width(Fill).center_y(Fill),
                action_surface,
            ]
            .spacing(8)
            .align_y(Center),
        )
        .padding([6, 10])
        .width(Fill)
        .height(COMPOSER_PENDING_QUEUE_ROW_HEIGHT)
        .style(move |theme| style::workbench_pending_prompt_queue_row_style(theme, is_drag_source))
        .into()
    }

    /// Builds the tail drop zone shown after the pending-prompt queue rows so drag reordering can land at the end.
    /// 构建显示在待发送队列行末尾的尾部落点，以便拖拽重排落到最后。
    pub(super) fn view_pending_prompt_queue_tail_drop_zone(&self) -> Element<'_, Message> {
        if self.dragging_pending_prompt_entry_id.is_none() {
            return space::vertical().height(0.0).into();
        }
        let shows_drop_tail = matches!(
            self.active_pending_prompt_drop_target.as_ref(),
            Some(PendingPromptDropTarget::End)
        );
        mouse_area(
            container(if shows_drop_tail {
                container(space())
                    .width(Fill)
                    .height(2.0)
                    .style(style::workbench_pending_prompt_drop_indicator_style)
            } else {
                container(space()).width(Fill).height(0.0)
            })
            .padding(iced::Padding {
                top: 2.0,
                right: 2.0,
                bottom: 0.0,
                left: 2.0,
            })
            .height(COMPOSER_PENDING_QUEUE_DROP_INDICATOR_HEIGHT)
            .width(Fill),
        )
        .interaction(iced::mouse::Interaction::Move)
        .on_enter(Message::PendingPromptQueueTailHovered(true))
        .on_exit(Message::PendingPromptQueueTailHovered(false))
        .into()
    }

    /// Builds the pending prompt queue panel that exposes queued follow-ups above the composer instead of mixing them into the transcript.
    /// 构建显示在提示词输入框上方的待发送队列面板，而不是把这些跟进混进转录正文。
    pub(super) fn view_pending_prompt_queue_strip(&self) -> Element<'_, Message> {
        let pending_entries = self.selected_session_pending_prompt_entries();
        if pending_entries.is_empty() {
            return space::vertical().height(0.0).into();
        }
        let pending_entry_count = pending_entries.len();
        let queue_body = pending_entries.into_iter().enumerate().fold(
            column![].spacing(0).width(Fill),
            |column, (index, entry)| {
                let shows_drop_before = matches!(
                    self.active_pending_prompt_drop_target.as_ref(),
                    Some(PendingPromptDropTarget::Before(target_entry_id))
                        if target_entry_id == &entry.id
                );
                let column = column
                    .push(self.view_pending_prompt_queue_drop_indicator(shows_drop_before))
                    .push(self.view_pending_prompt_queue_row(index, entry));
                if index + 1 < pending_entry_count {
                    column.push(space::vertical().height(COMPOSER_PENDING_QUEUE_ROW_GAP))
                } else {
                    column
                }
            },
        );
        let queue_body = queue_body.push(self.view_pending_prompt_queue_tail_drop_zone());
        container(container(queue_body).style(style::workbench_pending_prompt_queue_panel_style))
            .padding(iced::Padding {
                top: 0.0,
                right: COMPOSER_PENDING_QUEUE_PANEL_SIDE_INSET,
                bottom: 0.0,
                left: COMPOSER_PENDING_QUEUE_PANEL_SIDE_INSET,
            })
            .width(Fill)
            .into()
    }

    /// Builds the clickable workbench model trigger shown below the main prompt composer.
    /// 构建显示在主提示词编辑器下方的可点击工作台模型触发表面。
    pub(super) fn view_workbench_model_selector(&self) -> Element<'_, Message> {
        let selected_choice = self.active_workbench_model_choice();
        let palette = self.app_theme.extended_palette();
        let has_selected_choice = selected_choice.is_some();
        let model_label = selected_choice
            .as_ref()
            .map(|choice| choice.model_name.clone())
            .unwrap_or_else(|| String::from("选择模型"));
        let provider_label = selected_choice
            .as_ref()
            .map(|choice| choice.provider_label.clone())
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| String::from("选择供应商"));
        let has_choices = !self.configured_workbench_model_choices().is_empty();
        let icon = lucide::cpu()
            .size(14)
            .line_height(1.0)
            .color(palette.primary.base.color)
            .into();

        self.view_workbench_dual_line_trigger(
            Some(icon),
            model_label,
            provider_label,
            WORKBENCH_MODEL_SELECTOR_WIDTH,
            has_selected_choice,
            has_choices.then_some(Message::OpenWorkbenchModelPicker),
        )
    }

    /// Builds the clickable OpenRouter secondary-provider trigger shown only when the active workbench model belongs to OpenRouter.
    /// 构建仅在当前激活工作台模型属于 OpenRouter 时显示的可点击二级供应商触发表面。
    pub(super) fn view_workbench_openrouter_endpoint_selector(&self) -> Element<'_, Message> {
        let Some(selected_choice) = self.active_workbench_model_choice() else {
            return space::horizontal().width(0).into();
        };
        if selected_choice.provider_id != "openrouter" {
            return space::horizontal().width(0).into();
        }

        let palette = self.app_theme.extended_palette();
        let selected_route = self.active_workbench_openrouter_endpoint_route();
        let title = selected_route
            .as_ref()
            .map(|route| route.display_label())
            .unwrap_or_else(|| String::from("Auto"));
        let icon = lucide::route()
            .size(14)
            .line_height(1.0)
            .color(palette.primary.base.color)
            .into();

        self.view_workbench_dual_line_trigger(
            Some(icon),
            title,
            String::from("二级供应商"),
            WORKBENCH_OPENROUTER_ENDPOINT_SELECTOR_WIDTH,
            true,
            Some(Message::OpenWorkbenchOpenRouterEndpointPicker),
        )
    }

    /// Builds the blocking OpenRouter secondary-provider picker modal shown above the main workbench when the active model supports endpoint routing.
    /// 构建在当前模型支持端点路由时显示在主工作台之上的阻塞式 OpenRouter 二级供应商选择弹窗。
    pub(super) fn view_workbench_openrouter_endpoint_picker_modal(
        &self,
    ) -> Option<Element<'_, Message>> {
        if !self.workbench_openrouter_endpoint_picker.open {
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
        .on_press(Message::CloseWorkbenchOpenRouterEndpointPicker);
        let selected_route = self.active_workbench_openrouter_endpoint_route();
        let filtered_choices = self.filtered_workbench_openrouter_endpoint_choices();
        let option_list: Element<'_, Message> = if self.workbench_openrouter_endpoint_picker.loading
        {
            container(
                container(text("正在加载二级供应商列表...").size(14))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKBENCH_OPENROUTER_ENDPOINT_PICKER_LIST_HEIGHT)
            .into()
        } else if let Some(error) = self.workbench_openrouter_endpoint_picker.error.as_ref() {
            container(
                container(text(error.clone()).size(13))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKBENCH_OPENROUTER_ENDPOINT_PICKER_LIST_HEIGHT)
            .into()
        } else if filtered_choices.is_empty() {
            container(
                container(text("没有找到匹配的二级供应商。").size(13))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKBENCH_OPENROUTER_ENDPOINT_PICKER_LIST_HEIGHT)
            .into()
        } else {
            scrollable(
                column(filtered_choices.into_iter().map(|choice| {
                    let route = choice.to_route_selection();
                    let is_selected = selected_route.as_ref() == Some(&route);
                    button(
                        container(
                            column![
                                row![
                                    text(choice.provider_name.clone()).size(14),
                                    space::horizontal().width(Fill),
                                    container(text(choice.tag.clone()).size(11))
                                        .padding([3, 8])
                                        .style(style::main_task_search_shortcut_badge_style),
                                ]
                                .align_y(Center),
                                text(choice.subtitle_line())
                                    .size(12)
                                    .color(palette.background.base.text.scale_alpha(0.62)),
                            ]
                            .spacing(6)
                            .width(Fill),
                        )
                        .width(Fill),
                    )
                    .width(Fill)
                    .padding([10, 12])
                    .style(move |theme, status| {
                        style::main_task_search_result_button_style(theme, status, is_selected)
                    })
                    .on_press(Message::WorkbenchOpenRouterEndpointSelected(Some(route)))
                    .into()
                }))
                .spacing(8),
            )
            .height(WORKBENCH_OPENROUTER_ENDPOINT_PICKER_LIST_HEIGHT)
            .into()
        };
        let auto_selected = selected_route.is_none();
        let auto_button = button(
            container(
                column![
                    text("Auto").size(14),
                    text("自动路由（可能切换到不同上游供应商）")
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.62)),
                ]
                .spacing(6)
                .width(Fill),
            )
            .width(Fill),
        )
        .width(Fill)
        .padding([10, 12])
        .style(move |theme, status| {
            style::main_task_search_result_button_style(theme, status, auto_selected)
        })
        .on_press(Message::WorkbenchOpenRouterEndpointSelected(None));
        let modal_card = container(
            column![
                row![
                    text("选择二级供应商").size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text_input(
                    "按供应商名称、标签或端点过滤",
                    &self.workbench_openrouter_endpoint_picker.filter
                )
                .on_input(Message::WorkbenchOpenRouterEndpointPickerFilterChanged)
                .padding([10, 12]),
                auto_button,
                option_list,
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(WORKBENCH_OPENROUTER_ENDPOINT_PICKER_MODAL_WIDTH)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchOpenRouterEndpointPicker);

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

    /// Builds the compact reasoning-effort selector shown before the trailing send button.
    /// 构建显示在尾部发送按钮之前的紧凑推理级别选择器。
    pub(super) fn view_workbench_reasoning_effort_selector(&self) -> Element<'_, Message> {
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
        let selected_option = reasoning_view
            .as_ref()
            .and_then(|view| {
                view.options
                    .iter()
                    .find(|option| option.effort == view.selected_effort)
                    .cloned()
            })
            .or_else(|| reasoning_options.first().cloned());
        let title = selected_option
            .as_ref()
            .map(|option| option.label.clone())
            .unwrap_or_else(|| String::from("关闭"));
        let is_expandable = reasoning_options.len() > 1;

        self.view_workbench_reasoning_trigger(
            title,
            selected_option.is_some(),
            is_expandable,
            is_expandable.then_some(Message::OpenWorkbenchReasoningPicker),
        )
    }

    /// Builds the dedicated two-line reasoning trigger without reserving any leading icon gutter.
    /// 构建不预留左侧图标槽位的专用双行思考级别触发器。
    fn view_workbench_reasoning_trigger(
        &self,
        title: String,
        active: bool,
        shows_chevron: bool,
        on_press: Option<Message>,
    ) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let centered_title_column = container(
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
                    text("思考级别")
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
        .center(Fill);
        let chevron_overlay: Element<'_, Message> = if shows_chevron {
            container(
                row![
                    space::horizontal(),
                    codicon::chevron_down()
                        .size(14)
                        .line_height(1.0)
                        .color(palette.background.base.text.scale_alpha(0.48)),
                ]
                .align_y(Center)
                .width(Fill),
            )
            .width(Fill)
            .height(Fill)
            .center_y(Fill)
            .padding([0, 12])
            .into()
        } else {
            space().into()
        };
        let trigger = button(
            container(stack([centered_title_column.into(), chevron_overlay]))
                .width(Fill)
                .height(Fill),
        )
        .width(WORKBENCH_REASONING_SELECTOR_WIDTH)
        .height(COMPOSER_CONTROL_ROW_HEIGHT)
        .padding(0)
        .style(style::workbench_model_trigger_button_style);
        if let Some(message) = on_press {
            trigger.on_press(message).into()
        } else {
            trigger.into()
        }
    }

    /// Builds the dedicated attachment button shown beside the composer execution controls.
    /// 构建显示在提示词执行控制行旁侧的专用附件按钮。
    pub(super) fn view_composer_attachment_button(&self) -> Element<'_, Message> {
        let supports_attachments = self.active_workbench_supports_any_attachments();
        let button = button(
            container(
                lucide::image_plus()
                    .size(16)
                    .line_height(1.0)
                    .color(self.app_theme.extended_palette().background.base.text),
            )
            .width(COMPOSER_CONTROL_ROW_HEIGHT)
            .height(COMPOSER_CONTROL_ROW_HEIGHT)
            .center(Fill),
        )
        .padding(0)
        .width(COMPOSER_CONTROL_ROW_HEIGHT)
        .height(COMPOSER_CONTROL_ROW_HEIGHT)
        .style(style::workbench_model_trigger_button_style);
        let button = if supports_attachments {
            button.on_press(Message::AddComposerAttachments)
        } else {
            button
        };
        let tooltip_text = if supports_attachments {
            "添加附件（支持整块输入区拖拽、Ctrl+V 智能粘贴与 Ctrl+Shift+V 附件粘贴）"
        } else {
            "当前模型未声明可用附件输入模态"
        };

        tooltip(button, tooltip_text, tooltip::Position::Bottom)
            .style(container::rounded_box)
            .into()
    }

    /// Builds the staged composer attachment strip shown between the editor and the control row.
    /// 构建显示在编辑器与控制行之间的已暂存提示词附件条。
    pub(super) fn view_composer_attachment_strip(&self) -> Element<'_, Message> {
        if self.composer_attachments.is_empty() {
            return space::vertical().height(0.0).into();
        }

        let attachment_row = self
            .composer_attachments
            .iter()
            .fold(row![].spacing(8).align_y(Center), |row, attachment| {
                row.push(self.view_composer_attachment_tile(attachment))
            });
        let strip_body = row![
            container(
                text(format!(
                    "附件 {}/{}",
                    self.composer_attachments.len(),
                    MAX_COMPOSER_ATTACHMENTS
                ))
                .size(12),
            )
            .width(64)
            .center_y(COMPOSER_ATTACHMENT_ROW_HEIGHT),
            scrollable(container(attachment_row).width(Shrink))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default(),
                ))
                .height(COMPOSER_ATTACHMENT_ROW_HEIGHT)
                .width(Fill),
        ]
        .spacing(8)
        .align_y(Center)
        .height(COMPOSER_ATTACHMENT_ROW_HEIGHT)
        .width(Fill);

        container(strip_body)
            .height(COMPOSER_ATTACHMENT_ROW_HEIGHT)
            .width(Fill)
            .into()
    }

    /// Builds one square staged-attachment tile with an icon or image preview, hover filename tooltip, and circular remove button.
    /// 构建单个已暂存附件方格，包含图标或图片预览、悬浮文件名提示以及圆形移除按钮。
    fn view_composer_attachment_tile<'a>(
        &'a self,
        attachment: &'a ConversationAttachment,
    ) -> Element<'a, Message> {
        let remove_button = button(
            container(codicon::chrome_close().size(12).line_height(1.0))
                .width(COMPOSER_ATTACHMENT_REMOVE_BUTTON_SIZE)
                .height(COMPOSER_ATTACHMENT_REMOVE_BUTTON_SIZE)
                .center(Fill),
        )
        .padding(0)
        .width(COMPOSER_ATTACHMENT_REMOVE_BUTTON_SIZE)
        .height(COMPOSER_ATTACHMENT_REMOVE_BUTTON_SIZE)
        .style(style::workbench_composer_attachment_remove_button_style)
        .on_press(Message::RemoveComposerAttachment(
            attachment.local_path.clone(),
        ));
        let tile_body = container(self.view_composer_attachment_tile_body(attachment))
            .width(COMPOSER_ATTACHMENT_TILE_SIZE)
            .height(COMPOSER_ATTACHMENT_TILE_SIZE)
            .style(style::workbench_composer_attachment_tile_style);
        let tile_content: Element<'a, Message> =
            if matches!(attachment.kind, ConversationAttachmentKind::Image) {
                button(tile_body)
                    .padding(0)
                    .width(COMPOSER_ATTACHMENT_TILE_SIZE)
                    .height(COMPOSER_ATTACHMENT_TILE_SIZE)
                    .style(style::workbench_composer_attachment_preview_button_style)
                    .on_press(Message::OpenComposerImageAttachmentPreview {
                        file_name: attachment.file_name.clone(),
                        local_path: attachment.local_path.clone(),
                    })
                    .into()
            } else {
                tile_body.into()
            };
        let tile_surface = stack([
            tile_content,
            container(remove_button)
                .width(COMPOSER_ATTACHMENT_TILE_SIZE)
                .height(COMPOSER_ATTACHMENT_TILE_SIZE)
                .align_x(iced::Alignment::End)
                .align_y(iced::Alignment::Start)
                .into(),
        ])
        .width(COMPOSER_ATTACHMENT_TILE_SIZE)
        .height(COMPOSER_ATTACHMENT_TILE_SIZE);

        tooltip(
            tile_surface,
            text(attachment.file_name.clone()).size(12),
            tooltip::Position::Top,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Builds the visual body of one attachment tile, using real thumbnails for available images and semantic icons for other media.
    /// 构建单个附件方格的视觉主体，可用图片使用真实缩略图，其他媒体使用语义图标。
    fn view_composer_attachment_tile_body<'a>(
        &'a self,
        attachment: &'a ConversationAttachment,
    ) -> Element<'a, Message> {
        if matches!(attachment.kind, ConversationAttachmentKind::Image)
            && std::path::Path::new(&attachment.local_path).is_file()
        {
            return iced_image(attachment.local_path.clone())
                .width(Fill)
                .height(Fill)
                .content_fit(ContentFit::Cover)
                .border_radius(14.0)
                .into();
        }
        if matches!(attachment.kind, ConversationAttachmentKind::Video)
            && let Some(preview) = resolve_video_thumbnail_preview(
                &attachment.local_path,
                Some(attachment.mime_type.as_str()),
            )
        {
            return iced_image(preview.handle)
                .width(Fill)
                .height(Fill)
                .content_fit(ContentFit::Cover)
                .border_radius(14.0)
                .into();
        }

        let palette = self.app_theme.extended_palette();
        let icon_color = match attachment.kind {
            ConversationAttachmentKind::Image => palette.primary.base.color,
            ConversationAttachmentKind::Audio => iced::color!(0x0E7490),
            ConversationAttachmentKind::Video => iced::color!(0xB45309),
            ConversationAttachmentKind::Pdf => iced::color!(0xB91C1C),
        };
        let icon = match attachment.kind {
            ConversationAttachmentKind::Image => lucide::image(),
            ConversationAttachmentKind::Audio => lucide::volume_two(),
            ConversationAttachmentKind::Video => lucide::video(),
            ConversationAttachmentKind::Pdf => lucide::file_text(),
        }
        .size(26)
        .line_height(1.0)
        .color(icon_color);
        let kind_label = match attachment.kind {
            ConversationAttachmentKind::Image => "IMG",
            ConversationAttachmentKind::Audio => "AUDIO",
            ConversationAttachmentKind::Video => "VIDEO",
            ConversationAttachmentKind::Pdf => "PDF",
        };

        container(column![icon, text(kind_label).size(10).color(icon_color)].spacing(5))
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .style(style::workbench_composer_attachment_icon_tile_style)
            .into()
    }

    /// Builds the lightweight drag-hover hint shown above the composer editor while files are hovering over the main window.
    /// 构建当文件正悬停在主窗口上方时显示在提示词编辑器上方的轻量拖拽提示。
    pub(super) fn view_composer_drag_hover_hint(&self) -> Option<Element<'_, Message>> {
        if self.composer_drag_hover_labels.is_empty() {
            return None;
        }

        let preview = if self.composer_drag_hover_labels.len() == 1 {
            format!(
                "拖到整个输入区后释放以附加文件：{}",
                self.composer_drag_hover_labels[0]
            )
        } else {
            format!(
                "拖到整个输入区后释放以附加 {} 个文件：{}",
                self.composer_drag_hover_labels.len(),
                self.composer_drag_hover_labels
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("、")
            )
        };

        Some(
            container(text(preview).size(12))
                .padding([6, 10])
                .width(Fill)
                .style(container::rounded_box)
                .into(),
        )
    }

    /// Builds the inline slash-command suggestion panel shown under the composer when the current text still matches supported slash commands.
    /// 构建显示在提示词输入框下方、并在当前文本仍匹配受支持 slash 指令时可见的内联建议面板。
    pub(super) fn view_composer_slash_command_panel(&self) -> Option<Element<'_, Message>> {
        let state = self.composer_slash_command_panel_state()?;
        let palette = self.app_theme.extended_palette();
        let panel_height = (state.visible_commands.len() as f32
            * COMPOSER_SLASH_COMMAND_ROW_HEIGHT)
            .min(COMPOSER_SLASH_COMMAND_PANEL_MAX_HEIGHT);
        let panel_title = if state.filter_text.is_empty() {
            String::from("Slash 指令")
        } else {
            format!("Slash 指令 · {}", state.filter_text)
        };
        let command_rows = column(state.visible_commands.iter().enumerate().map(
            |(index, command)| {
                let is_selected = state.highlighted_index == Some(index);
                button(
                    container(
                        row![
                            container(text(command.trigger()).size(13).color(if is_selected {
                                palette.primary.strong.color
                            } else {
                                palette.background.base.text
                            }),)
                            .width(96),
                            column![
                                text(command.summary()).size(13),
                                text("上下选择后回车，或完整输入后直接回车激活")
                                    .size(11)
                                    .color(palette.background.base.text.scale_alpha(0.56)),
                            ]
                            .spacing(2)
                            .width(Fill),
                        ]
                        .spacing(10)
                        .align_y(Center),
                    )
                    .width(Fill)
                    .center_y(Fill),
                )
                .width(Fill)
                .height(COMPOSER_SLASH_COMMAND_ROW_HEIGHT)
                .padding([0, 8])
                .style(move |theme, status| {
                    style::workbench_model_picker_option_button_style(theme, status, is_selected)
                })
                .on_press(Message::ActivateComposerSlashCommand(*command))
                .into()
            },
        ))
        .spacing(2);

        Some(
            container(
                column![
                    text(panel_title)
                        .size(12)
                        .color(palette.background.base.text.scale_alpha(0.70)),
                    container(command_rows).height(panel_height),
                ]
                .spacing(6),
            )
            .padding([8, 10])
            .width(Fill)
            .style(style::main_task_tree_menu_style)
            .into(),
        )
    }

    /// Returns the current estimated visible line count of the main prompt composer.
    /// 返回主提示词编辑器当前估算得到的可见行数量。
    pub(super) fn composer_line_count(&self) -> usize {
        composer_visible_line_count(&self.composer_buffer, self.composer_input_surface_width())
    }

    /// Returns the current dynamic height of the main prompt composer editor.
    /// 返回主提示词编辑器当前的动态高度。
    pub(super) fn composer_editor_height(&self) -> f32 {
        let measured_text_height = composer_measured_text_height(
            &self.composer_buffer,
            self.composer_input_surface_width(),
        );
        let max_visible_text_height = COMPOSER_EDITOR_LINE_HEIGHT * 10.0;
        let visible_text_height = measured_text_height.min(max_visible_text_height);

        COMPOSER_EDITOR_VERTICAL_INSET + visible_text_height
    }

    /// Returns the current content height, viewport height, and vertical scroll offset of the main prompt composer editor.
    /// 返回主提示词编辑器当前的内容高度、视口高度与纵向滚动偏移量。
    pub(super) fn composer_editor_scroll_metrics(&self) -> (f32, f32, f32) {
        let content_height = COMPOSER_EDITOR_VERTICAL_INSET
            + composer_measured_text_height(
                &self.composer_buffer,
                self.composer_input_surface_width(),
            );
        let viewport_height = self.composer_editor_height();

        (
            content_height,
            viewport_height,
            self.composer_scroll_offset.max(0.0),
        )
    }

    /// Returns the centered composer-panel width budget derived from the current main workbench canvas.
    /// 返回根据当前主工作台画布推导出的居中提示词面板宽度预算。
    pub(super) fn composer_panel_width(&self) -> f32 {
        let left_region_width = if self.main_task_panel_expanded {
            self.main_task_panel_width + MAIN_TASK_PANEL_RESIZE_HANDLE_WIDTH
        } else {
            MAIN_TASK_PANEL_RESTORE_RAIL_WIDTH
        };
        let chrome_width = MAIN_PANEL_ACTIVITY_BAR_WIDTH + left_region_width + 36.0;
        let canvas_width = (self.main_window_size.width - chrome_width).max(420.0);

        (canvas_width * 0.80).max(420.0)
    }

    /// Returns the actual left edge of the floating composer panel inside the right workbench canvas.
    /// 返回浮动输入面板在右侧工作台画布中的实际左边界。
    pub(super) fn composer_panel_left(&self) -> f32 {
        let left_region_width = if self.main_task_panel_expanded {
            self.main_task_panel_width + MAIN_TASK_PANEL_RESIZE_HANDLE_WIDTH
        } else {
            MAIN_TASK_PANEL_RESTORE_RAIL_WIDTH
        };
        let chrome_width = MAIN_PANEL_ACTIVITY_BAR_WIDTH + left_region_width + 36.0;
        let canvas_width = (self.main_window_size.width - chrome_width).max(420.0);
        let composer_panel_width = self.composer_panel_width();

        chrome_width + ((canvas_width - composer_panel_width) / 2.0).max(0.0)
    }

    /// Returns the floating composer footprint currently covering the bottom of the transcript viewport.
    /// 返回当前覆盖在转录视口底部的浮动提示词区域占用高度。
    pub(super) fn floating_composer_footprint_height(&self) -> f32 {
        let visible_editor_height = if self.composer_uses_expanded_surface() {
            self.composer_editor_height()
        } else {
            COMPOSER_SINGLE_LINE_HEIGHT
        };
        let attachment_strip_height = if self.composer_attachments.is_empty() {
            0.0
        } else {
            COMPOSER_ATTACHMENT_ROW_HEIGHT + COMPOSER_PANEL_CONTROL_GAP
        };
        let pending_queue_strip_height = if self.selected_subagent_task_panel_state().is_none()
            && self.should_show_selected_session_pending_prompt_strip()
        {
            self.selected_session_pending_prompt_panel_height()
        } else {
            0.0
        };

        pending_queue_strip_height
            + visible_editor_height
            + attachment_strip_height
            + COMPOSER_CONTROL_ROW_HEIGHT
            + COMPOSER_PANEL_CONTROL_GAP
            + (COMPOSER_PANEL_EDGE_PADDING as f32 * 2.0)
    }

    /// Returns the transcript tail spacer needed to keep the latest content clear of the floating composer.
    /// 返回用于让最新转录内容避开浮动提示词区域的尾部留白高度。
    pub(super) fn transcript_bottom_spacer_height(&self) -> f32 {
        TRANSCRIPT_BOTTOM_SPACER_HEIGHT + self.floating_composer_footprint_height()
    }

    /// Returns the actual inner input-surface width after subtracting the composer-panel horizontal padding.
    /// 返回扣除提示词面板水平内边距后的实际输入表面宽度。
    pub(super) fn composer_input_surface_width(&self) -> f32 {
        self.composer_panel_width() - (COMPOSER_PANEL_EDGE_PADDING as f32 * 2.0)
    }

    /// Returns whether the main prompt composer should expand upward as a multi-line surface.
    /// 返回主提示词编辑器当前是否应作为向上展开的多行输入表面显示。
    pub(super) fn composer_uses_expanded_surface(&self) -> bool {
        self.composer_line_count() > 1
    }

    /// Builds the shared main prompt composer editor surface.
    /// 构建主提示词编辑器共用的实际输入表面。
    pub(super) fn view_composer_editor_surface(&self) -> Element<'_, Message> {
        let visible_height = self.composer_editor_height();
        let can_submit = self.composer_has_submittable_content();
        let slash_panel_visible = self.composer_slash_command_panel_state().is_some();
        let highlighted_slash_command = self.highlighted_composer_slash_command();
        let exact_slash_command = self.exact_composer_slash_command();

        let editor = text_editor(&self.composer_buffer)
            .id(COMPOSER_EDITOR_ID)
            .height(visible_height)
            .size(COMPOSER_EDITOR_TEXT_SIZE)
            .line_height(iced::widget::text::LineHeight::Absolute(Pixels(
                COMPOSER_EDITOR_LINE_HEIGHT,
            )))
            .wrapping(iced::advanced::text::Wrapping::WordOrGlyph)
            .padding([10, 12])
            .placeholder("输入当前会话要执行的任务指令")
            .style(style::workbench_composer_editor_style)
            .key_binding(move |key_press| match key_press.key.as_ref() {
                keyboard::Key::Character("z") if key_press.modifiers.command() => {
                    Some(text_editor::Binding::Custom(Message::ComposerUndo))
                }
                keyboard::Key::Character("v") | keyboard::Key::Character("V")
                    if key_press.modifiers.command() && key_press.modifiers.shift() =>
                {
                    Some(text_editor::Binding::Custom(
                        Message::PasteComposerClipboardAttachments,
                    ))
                }
                keyboard::Key::Character("v") | keyboard::Key::Character("V")
                    if key_press.modifiers.command()
                        && !key_press.modifiers.shift()
                        && !key_press.modifiers.alt()
                        && !key_press.modifiers.logo() =>
                {
                    Some(text_editor::Binding::Custom(
                        Message::PasteComposerClipboardContents,
                    ))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) if slash_panel_visible => Some(
                    text_editor::Binding::Custom(Message::ComposerSlashCommandMoveSelectionUp),
                ),
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) if slash_panel_visible => {
                    Some(text_editor::Binding::Custom(
                        Message::ComposerSlashCommandMoveSelectionDown,
                    ))
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) if slash_panel_visible => Some(
                    text_editor::Binding::Custom(Message::DismissComposerSlashCommandPanel),
                ),
                keyboard::Key::Named(keyboard::key::Named::Enter)
                    if !key_press.modifiers.shift() =>
                {
                    if let Some(command) = exact_slash_command.or(highlighted_slash_command) {
                        Some(text_editor::Binding::Custom(
                            Message::ActivateComposerSlashCommand(command),
                        ))
                    } else if can_submit {
                        Some(text_editor::Binding::Custom(Message::StartRun))
                    } else {
                        Some(text_editor::Binding::Sequence(Vec::new()))
                    }
                }
                _ => text_editor::Binding::from_key_press(key_press),
            })
            .on_action(Message::ComposerEditorAction);

        let scrollbar_overlay: Element<'_, Message> =
            if let Some(layout) = self.composer_scrollbar_layout() {
                let thumb_bottom_space =
                    (layout.track_height - layout.thumb_top - layout.thumb_height).max(0.0);
                let scrollbar_track = mouse_area(
                    container(
                        column![
                            space().height(layout.thumb_top),
                            container(space())
                                .width(6)
                                .height(layout.thumb_height)
                                .style(style::workbench_composer_editor_scrollbar_style),
                            space().height(thumb_bottom_space),
                        ]
                        .spacing(0)
                        .width(6)
                        .height(layout.track_height)
                        .align_x(iced::Alignment::End),
                    )
                    .width(10)
                    .height(layout.track_height),
                )
                .interaction(if self.composer_scrollbar_drag_grab_offset.is_some() {
                    iced::mouse::Interaction::Grabbing
                } else {
                    iced::mouse::Interaction::Grab
                })
                .on_move(|position| Message::ComposerScrollbarPointerMoved(position.y))
                .on_press(Message::StartComposerScrollbarDrag);

                container(scrollbar_track)
                    .padding(iced::Padding {
                        top: 8.0,
                        right: 4.0,
                        bottom: 8.0,
                        left: 0.0,
                    })
                    .width(Fill)
                    .height(visible_height)
                    .align_x(iced::Alignment::End)
                    .center_y(Fill)
                    .into()
            } else {
                container(space::vertical().height(0)).into()
            };

        container(
            mouse_area(stack([
                container(editor).width(Fill).into(),
                scrollbar_overlay,
            ]))
            .on_right_press(Message::OpenComposerContextMenu),
        )
        .width(Fill)
        .height(visible_height)
        .clip(true)
        .into()
    }

    /// Builds the floating overlay used by the shared workbench context menu.
    /// 构建共享工作台右键菜单使用的浮动覆盖层。
    pub(super) fn view_workbench_context_menu_overlay(&self) -> Option<Element<'_, Message>> {
        let menu_state = self.workbench_context_menu.as_ref()?;
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseWorkbenchContextMenu)
            .on_right_press(Message::CloseWorkbenchContextMenu);
        let menu_position = self.workbench_context_menu_position(menu_state);
        let menu = pin(self.view_workbench_context_menu())
            .position(menu_position)
            .width(Fill)
            .height(Fill);

        Some(
            opaque(
                stack([backdrop.into(), menu.into()])
                    .width(Fill)
                    .height(Fill),
            )
            .into(),
        )
    }

    /// Returns the bounded window-relative position used by the shared context menu overlay.
    /// 返回共享右键菜单覆盖层使用的受边界约束窗口相对位置。
    pub(super) fn workbench_context_menu_position(
        &self,
        menu_state: &WorkbenchContextMenuState,
    ) -> iced::Point {
        let row_count = menu_state.items.len() as f32;
        let gap_count = menu_state.items.len().saturating_sub(1) as f32;
        let menu_width = menu_state.width;
        let menu_height =
            (WORKBENCH_CONTEXT_MENU_ROW_HEIGHT * row_count) + (2.0 * gap_count) + 12.0;
        let window_width = self.main_window_size.width.max(menu_width + 12.0);
        let window_height = self.main_window_size.height.max(menu_height + 12.0);

        iced::Point::new(
            menu_state
                .anchor
                .x
                .clamp(8.0, (window_width - menu_width - 8.0).max(8.0)),
            menu_state
                .anchor
                .y
                .clamp(8.0, (window_height - menu_height - 8.0).max(8.0)),
        )
    }

    /// Builds the menu body used by the shared workbench context menu overlay.
    /// 构建共享工作台右键菜单覆盖层使用的菜单主体。
    pub(super) fn view_workbench_context_menu(&self) -> Element<'_, Message> {
        let Some(menu_state) = self.workbench_context_menu.as_ref() else {
            return container(space::vertical().height(0)).into();
        };
        let items = menu_state
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| self.view_workbench_context_menu_button(index, item))
            .collect::<Vec<_>>();

        container(column(items).spacing(2).width(menu_state.width))
            .padding([6, 6])
            .width(menu_state.width)
            .style(style::main_task_tree_menu_style)
            .into()
    }

    /// Builds one compact menu button used inside the shared context menu overlay.
    /// 构建共享右键菜单覆盖层内部使用的一条紧凑菜单按钮。
    pub(super) fn view_workbench_context_menu_button(
        &self,
        index: usize,
        item: &WorkbenchContextMenuItem,
    ) -> Element<'_, Message> {
        let text_alpha = if item.message.is_some() { 0.92 } else { 0.38 };
        let label = item.label.clone();
        let mut button = button(
            container(text(label).size(13).style(move |theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(text_alpha),
                    ),
                }
            }))
            .padding([6, 10])
            .width(Fill)
            .height(WORKBENCH_CONTEXT_MENU_ROW_HEIGHT),
        )
        .padding(0)
        .width(Fill)
        .style(style::main_task_tree_menu_button_style);

        if item.message.is_some() {
            button = button.on_press(Message::InvokeWorkbenchContextMenuItem(index));
        }

        button.into()
    }

    /// Builds the inline composer editor slot used inside the floating composer panel itself.
    /// 构建直接放置在浮动提示词面板内部的内联编辑器区域。
    pub(super) fn view_inline_composer_editor(&self) -> Element<'_, Message> {
        self.view_composer_editor_surface()
    }

    /// Builds the compact context-usage orb shown below the floating composer editor.
    /// 构建显示在悬浮输入编辑器下方的紧凑上下文占用圆盘。
    pub(super) fn view_composer_context_usage_orb(&self) -> Element<'_, Message> {
        context_usage_orb::view_context_usage_orb(self.build_composer_context_usage_orb_props())
    }

    /// Builds the owned orb props consumed by the reusable context-usage widget.
    /// 构建供可复用上下文占用部件消费的自有属性集合。
    fn build_composer_context_usage_orb_props(&self) -> ContextUsageOrbProps {
        let tone = self.resolve_composer_context_usage_orb_tone();
        ContextUsageOrbProps {
            percent: self.context_usage,
            tooltip_title: String::from("上下文占用"),
            tooltip_rows: self.composer_context_usage_diagnostic_rows(),
            tone,
            diameter: 40.0,
        }
    }

    /// Builds the detailed tooltip rows shown by the floating composer context orb.
    /// 构建悬浮输入区上下文圆盘展示的详细提示行。
    fn composer_context_usage_diagnostic_rows(&self) -> Vec<String> {
        let selected_session_id = self.selected_session().map(|session| session.id);
        let runtime_state =
            selected_session_id.and_then(|session_id| self.session_runtime_state.get(&session_id));
        let preparation = self
            .last_execution_preparation
            .as_ref()
            .filter(|preparation| selected_session_id == Some(preparation.session_id));
        let context_budget = self.visible_context_budget_snapshot.as_ref();
        let mut request_breakdown_rows = self.composer_context_layer_usage_rows();
        let mut diagnostic_rows = Vec::new();

        diagnostic_rows.push(format!(
            "当前占用（输入）：{:.0}%",
            self.context_usage.clamp(0.0, 100.0)
        ));

        if let Some(context_budget) = context_budget {
            let has_observed_usage = context_budget.observed_usage.is_some();
            let input_source_label = if has_observed_usage {
                "API实测"
            } else {
                "本地估算"
            };
            let total_source_label = if has_observed_usage {
                "实测输入+预留"
            } else {
                "估算输入+预留"
            };
            let status_label = match context_budget.severity {
                ContextBudgetSeverity::Healthy => "正常",
                ContextBudgetSeverity::Warning => "预警",
                ContextBudgetSeverity::CompactSoon => "建议压缩",
                ContextBudgetSeverity::Overflow => "预算溢出",
            };
            diagnostic_rows.push(format!("状态：{}", status_label));
            diagnostic_rows.push(format!(
                "输入预算（{}）：{}",
                input_source_label,
                context_budget.usage_label()
            ));
            if request_breakdown_rows.is_empty() {
                diagnostic_rows.extend(Self::context_budget_breakdown_rows(context_budget));
            } else {
                diagnostic_rows.append(&mut request_breakdown_rows);
            }
            diagnostic_rows.push(format!(
                "预留：输出 {} · 推理 {} · 压缩 {}",
                context_budget.reserved_output_tokens,
                context_budget.reserved_reasoning_tokens,
                context_budget.reserved_compaction_tokens
            ));
            diagnostic_rows.push(format!(
                "端到端占用（{}）：{} tokens",
                total_source_label, context_budget.estimated_total_tokens
            ));
            if !context_budget.baseline_matches_active_runtime {
                diagnostic_rows.push(format!(
                    "恢复状态：基线失配（{}），已回退完整上下文并等待新锚点。",
                    context_budget
                        .baseline_mismatch_reason
                        .as_deref()
                        .unwrap_or("原因未记录")
                ));
            }
            if let Some(observed_usage) = context_budget.observed_usage.as_ref() {
                diagnostic_rows.push(format!(
                    "最近 API：输入 {} / 输出 {} / 推理 {} / 总计 {}",
                    observed_usage
                        .input_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| String::from("?")),
                    observed_usage
                        .output_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| String::from("?")),
                    observed_usage
                        .reasoning_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| String::from("?")),
                    observed_usage
                        .total_tokens
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| String::from("?")),
                ));
            }
        } else {
            diagnostic_rows.append(&mut request_breakdown_rows);
        }

        if let Some(runtime_state) = runtime_state {
            diagnostic_rows.push(if runtime_state.last_compaction_id.is_some() {
                String::from("压缩锚点：已建立")
            } else {
                String::from("压缩锚点：尚未建立")
            });
        }

        if let Some(preparation) = preparation {
            if let Some(context_baseline) = preparation.context_baseline.as_ref() {
                diagnostic_rows.push(format!("当前基线：{} 层", context_baseline.layer_count));
            }
        }

        diagnostic_rows
    }

    /// Resolves the context-orb visual tone and short status label from the latest visible budget state.
    /// 根据最近一次可见预算状态解析上下文圆盘的视觉主题与简短状态标签。
    fn resolve_composer_context_usage_orb_tone(&self) -> ContextUsageOrbTone {
        match self.visible_context_budget_snapshot.as_ref() {
            Some(context_budget) if !context_budget.baseline_matches_active_runtime => {
                ContextUsageOrbTone {
                    background: iced::color!(0x9A3412),
                    border: iced::color!(0xFECBA1),
                    track: iced::color!(0xFECBA1),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                }
            }
            Some(context_budget) => match context_budget.severity {
                ContextBudgetSeverity::Healthy => ContextUsageOrbTone {
                    background: iced::color!(0x1F7A5C),
                    border: iced::color!(0xA7F3D0),
                    track: iced::color!(0x6EE7B7),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::Warning => ContextUsageOrbTone {
                    background: iced::color!(0xA16207),
                    border: iced::color!(0xFDE68A),
                    track: iced::color!(0xFCD34D),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::CompactSoon => ContextUsageOrbTone {
                    background: iced::color!(0xB45309),
                    border: iced::color!(0xFDBA74),
                    track: iced::color!(0xFB923C),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::Overflow => ContextUsageOrbTone {
                    background: iced::color!(0xB42318),
                    border: iced::color!(0xFDA29B),
                    track: iced::color!(0xFEA3B4),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
            },
            None => ContextUsageOrbTone {
                background: iced::color!(0x475467),
                border: iced::color!(0xD0D5DD),
                track: iced::color!(0x98A2B3),
                progress: iced::color!(0x60A5FA),
                text_primary: Color::WHITE,
            },
        }
    }

    /// Builds one fallback local-estimate breakdown from the budget snapshot when no request assembly is available.
    /// 当没有请求装配信息时，根据预算快照构建一份兜底本地估算拆分。
    fn context_budget_breakdown_rows(context_budget: &ContextBudgetSnapshot) -> Vec<String> {
        let local_estimate_total = context_budget
            .estimated_system_tokens
            .saturating_add(context_budget.estimated_history_tokens)
            .saturating_add(context_budget.estimated_prompt_tokens)
            .saturating_add(context_budget.estimated_tool_tokens);
        Self::context_usage_part_rows(
            "估算拆分（参考）",
            vec![
                (String::from("系统"), context_budget.estimated_system_tokens),
                (
                    String::from("历史"),
                    context_budget.estimated_history_tokens,
                ),
                (String::from("当前"), context_budget.estimated_prompt_tokens),
                (String::from("工具"), context_budget.estimated_tool_tokens),
                (String::from("合计"), local_estimate_total),
            ],
        )
    }

    /// Builds compact tooltip rows from named token-estimate parts.
    /// 根据命名 Token 估算分项构建紧凑提示行。
    fn context_usage_part_rows(prefix: &str, parts: Vec<(String, usize)>) -> Vec<String> {
        let labels = parts
            .into_iter()
            .filter(|(_, tokens)| *tokens > 0usize)
            .map(|(label, tokens)| format!("{label} {tokens}"))
            .collect::<Vec<_>>();
        labels
            .chunks(3)
            .enumerate()
            .map(|(index, chunk)| {
                if index == 0usize {
                    format!("{}：{}", prefix, chunk.join(" · "))
                } else {
                    format!("{}（续）：{}", prefix, chunk.join(" · "))
                }
            })
            .collect()
    }

    /// Builds aggregated request-layer estimate rows reused by the context-orb tooltip.
    /// 构建供上下文圆盘提示层复用的请求层聚合估算行。
    fn composer_context_layer_usage_rows(&self) -> Vec<String> {
        let selected_session_id = self.selected_session().map(|session| session.id);
        let Some(context_assembly) = self
            .last_execution_preparation
            .as_ref()
            .filter(|preparation| selected_session_id == Some(preparation.session_id))
            .and_then(|preparation| preparation.context_assembly.as_ref())
        else {
            return Vec::new();
        };

        let mut parts = Vec::new();
        let mut estimate_total = 0usize;
        for kind in [
            ContextLayerKind::System,
            ContextLayerKind::ProjectRules,
            ContextLayerKind::Runtime,
            ContextLayerKind::History,
            ContextLayerKind::Retrieval,
            ContextLayerKind::Compaction,
            ContextLayerKind::Tools,
            ContextLayerKind::User,
        ] {
            let token_total = context_assembly
                .layers
                .iter()
                .filter(|layer| layer.kind == kind)
                .map(|layer| layer.token_estimate())
                .sum::<usize>();
            if token_total > 0 {
                estimate_total = estimate_total.saturating_add(token_total);
                parts.push((kind.label().to_string(), token_total));
            }
        }
        if estimate_total > 0usize {
            parts.push((String::from("合计"), estimate_total));
        }

        Self::context_usage_part_rows("请求拆分（本地估算）", parts)
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

    /// Renders the standalone provider-configuration card used by the dedicated settings workspace.
    /// 渲染独立设置工作区使用的供应商配置卡片。
    pub(super) fn view_provider_configuration_card(&self) -> Element<'_, Message> {
        let configured_providers = self.configured_provider_choices();
        let configured_provider_list: Element<'_, Message> = if configured_providers.is_empty() {
            container(
                column![
                    text("还没有模型供应商。").size(13),
                    text("先点击右上角“新增供应商”完成接入配置。保存后会进入“调整模型列表”草稿页，默认勾选供应商声明的推荐模型，其余模型需要你显式保存后才会生效。").size(12),
                ]
                .spacing(6),
            )
            .padding(12)
            .style(container::bordered_box)
            .into()
        } else {
            configured_providers
                .into_iter()
                .fold(column![].spacing(10), |list, provider| {
                    let provider_id = provider.id.clone();
                    let provider_name = provider.name.clone();
                    let definition = provider_config::supported_provider_by_id(&provider_id);
                    let entry = self.provider_configuration.provider.get(&provider_id);
                    let catalog_models = provider_config::catalog_model_choices(
                        &provider_id,
                        &self.provider_configuration,
                    );
                    let endpoint_summary = definition
                        .and_then(|definition| {
                            entry.and_then(|entry| definition.resolved_base_url(&entry.options))
                        })
                        .unwrap_or_else(|| String::from("由运行时自动解析"));
                    let api_key_summary = if entry
                        .map(|entry| {
                            entry
                                .options
                                .api_key
                                .as_deref()
                                .is_some_and(|api_key| !api_key.trim().is_empty())
                        })
                        .unwrap_or(false)
                    {
                        String::from("API Key：已填写")
                    } else {
                        String::from("API Key：未填写")
                    };
                    let enabled_model_count = provider_config::effective_model_choices(
                        &provider_id,
                        &self.provider_configuration,
                    )
                    .len();
                    let source_summary = definition
                        .map(|_| {
                            format!(
                                "{} · 内置目录 {} 个 · 当前启用 {} 个",
                                api_key_summary,
                                catalog_models.len(),
                                enabled_model_count
                            )
                        })
                        .unwrap_or_else(|| String::from("未找到供应商定义"));

                    let mut action_row = row![
                        button("编辑")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, false)
                            })
                            .on_press(Message::OpenProviderEditorForEdit(provider_id.clone())),
                        button("调整模型列表")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, false)
                            })
                            .on_press(Message::OpenProviderModelPicker(provider_id.clone())),
                    ]
                    .spacing(10);
                    action_row = action_row.push(
                        button("删除")
                            .padding([4, 10])
                            .style(|theme, status| {
                                style::workbench_label_action_button_style(theme, status, true)
                            })
                            .on_press(Message::RemoveConfiguredProvider(provider_id.clone())),
                    );

                    list.push(
                        container(
                            column![
                                row![
                                    column![
                                        text(provider_name).size(16),
                                        text(format!(
                                            "{source_summary} · 接口地址：{endpoint_summary}"
                                        ))
                                        .size(12)
                                        .color(
                                            self.app_theme
                                                .extended_palette()
                                                .background
                                                .base
                                                .text
                                                .scale_alpha(0.62),
                                        ),
                                    ]
                                    .spacing(6),
                                    space::horizontal(),
                                    action_row,
                                ]
                                .align_y(Center)
                                .spacing(10),
                            ]
                            .spacing(10),
                        )
                        .padding(12)
                        .style(container::bordered_box),
                    )
                })
                .into()
        };

        scrollable(container(configured_provider_list).width(Fill))
            .height(Fill)
            .spacing(8)
            .width(Fill)
            .into()
    }

    /// Renders the standalone enabled-model management page grouped by provider.
    /// 渲染按供应商分组展示的独立已启用模型管理页面。
    pub(super) fn view_provider_model_list_card(&self) -> Element<'_, Message> {
        let configured_providers = self.configured_provider_choices();
        let Some(active_provider) = self.selected_model_list_provider_choice() else {
            return column![text("当前还没有可展示的供应商。请先在“模型供应商”里新增至少一个供应商，然后再配置启用模型。").size(12)]
                .spacing(12)
                .width(Fill)
                .height(Fill)
                .into();
        };

        let provider_list = scrollable(configured_providers.into_iter().fold(
            column![].spacing(1).width(Fill),
            |list, provider| {
                let enabled_count = provider_config::effective_model_choices(
                    &provider.id,
                    &self.provider_configuration,
                )
                .len();
                let trailing_badge: Element<'_, Message> = text(format!("[{}]", enabled_count))
                    .size(12)
                    .color(
                        self.app_theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.56),
                    )
                    .into();
                list.push(self.view_tree_navigation_button_row(
                    self.view_settings_navigation_icon(SettingsSection::ModelProviders, true),
                    provider.name.clone(),
                    None,
                    Some(trailing_badge),
                    active_provider.id == provider.id,
                    MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                    Message::ProviderCatalogSelected(provider),
                ))
            },
        ))
        .height(Fill);

        let enabled_models = provider_config::effective_model_choices(
            &active_provider.id,
            &self.provider_configuration,
        );
        let enabled_model_list: Element<'_, Message> = if enabled_models.is_empty() {
            container(
                column![
                    text("当前供应商还没有启用任何模型。").size(13),
                    text(
                        "点击右上角“调整模型列表”进入完整目录草稿页，勾选后点击保存才会真正生效。"
                    )
                    .size(12),
                ]
                .spacing(6),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
        } else {
            scrollable(
                enabled_models
                    .into_iter()
                    .fold(column![].spacing(10), |list, model| {
                        let modality_summary = if model.input_modalities.is_empty() {
                            String::from("text")
                        } else {
                            model.input_modalities.join(" / ")
                        };
                        let summary = format!(
                            "{} · 上下文 {} · 输出 {}",
                            modality_summary,
                            model
                                .context_limit
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| String::from("-")),
                            model
                                .output_limit
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| String::from("-")),
                        );

                        list.push(
                            container(
                                row![
                                    column![
                                        text(model.name.clone()).size(13),
                                        text(format!("{} · {}", model.id, summary)).size(11).color(
                                            self.app_theme
                                                .extended_palette()
                                                .background
                                                .base
                                                .text
                                                .scale_alpha(0.58),
                                        ),
                                    ]
                                    .spacing(4)
                                    .width(Fill),
                                    button("编辑能力").style(button::secondary).on_press(
                                        Message::OpenProviderModelOverrideEditor {
                                            provider_id: active_provider.id.clone(),
                                            model_id: model.id.clone(),
                                        }
                                    ),
                                ]
                                .align_y(Center)
                                .spacing(10),
                            )
                            .padding(10)
                            .style(container::bordered_box),
                        )
                    }),
            )
            .height(Fill)
            .into()
        };

        row![
            column![text("供应商").size(13), provider_list]
                .spacing(8)
                .width(FillPortion(2))
                .height(Fill),
            column![
                row![
                    text("已启用模型").size(13),
                    space::horizontal(),
                    text(provider_config::provider_summary(&active_provider.id)).size(12),
                ]
                .align_y(Center),
                enabled_model_list,
            ]
            .spacing(10)
            .width(FillPortion(5))
            .height(Fill),
        ]
        .spacing(16)
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// Renders the blocking provider editor modal used for creating or updating one provider entry.
    /// 渲染用于创建或更新单条供应商配置的阻塞式供应商编辑弹窗。
    pub(super) fn view_provider_editor_modal(&self) -> Option<Element<'_, Message>> {
        let editor = self.provider_editor.as_ref()?;
        let palette = self.app_theme.extended_palette();
        let provider_choices = provider_config::supported_provider_choices();
        let selected_definition = self.selected_provider_definition();
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
        .on_press(Message::CloseProviderEditor);
        let provider_picker: Element<'_, Message> = if editor.is_new {
            pick_list(
                provider_choices,
                Some(self.selected_provider.clone()),
                Message::ProviderCatalogSelected,
            )
            .width(Fill)
            .placeholder("选择供应商")
            .into()
        } else {
            container(text(self.selected_provider.name.clone()).size(14))
                .padding(10)
                .style(container::bordered_box)
                .width(Fill)
                .into()
        };
        let provider_metadata: Element<'_, Message> = selected_definition.map_or_else(
            || text("当前没有可用的供应商定义。").size(12).into(),
            |definition| {
                column![
                    text(format!("运行时：{}", definition.runtime.label())).size(12),
                    text(format!(
                        "接口族：{} · {}",
                        definition.runtime.api_family().label(),
                        definition.runtime.api_family().key()
                    ))
                    .size(12),
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
            },
        );
        let provider_field_list = selected_definition.map_or_else(
            || column![text("当前没有可用的供应商定义。").size(13)].spacing(8),
            |definition| {
                definition
                    .input_fields()
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
                        form.push(column![text(label).size(13), input].spacing(6))
                    })
            },
        );
        let modal_card =
            container(
                column![
                row![
                    text(if editor.is_new {
                        "新增模型供应商"
                    } else {
                        "编辑模型供应商"
                    })
                    .size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text("这里只保存供应商接入配置；启用模型与停用模型请进入“调整模型列表”弹窗后显式保存。")
                    .size(12),
                text("供应商").size(13),
                provider_picker,
                provider_metadata,
                provider_field_list,
                row![
                    text(if editor.is_new {
                        "保存后会打开“调整模型列表”弹窗，并按该供应商内置目录里的默认勾选初始化草稿。"
                    } else {
                        "保存只更新当前供应商接入配置，不会自动改动已启用模型集合。"
                    })
                    .size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderConfiguration),
                ]
                .align_y(Center),
            ]
                .spacing(14),
            )
            .padding([22, 24])
            .width(760)
            .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderEditor);

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

    /// Renders the blocking provider model-picker modal used to edit one provider's enabled-model draft before explicit save.
    /// 渲染用于在显式保存前编辑某个供应商启用模型草稿的阻塞式供应商模型配置弹窗。
    pub(super) fn view_provider_model_picker_modal(&self) -> Option<Element<'_, Message>> {
        let picker = self.provider_model_picker.as_ref()?;
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
        .on_press(Message::CloseProviderModelPicker);
        let current_draft_enabled_ids = picker
            .draft_enabled_model_ids_by_provider
            .get(&picker.selected_provider_id)
            .cloned()
            .unwrap_or_default();
        let total_draft_enabled_count = picker
            .draft_enabled_model_ids_by_provider
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        let provider_list = scrollable(
            self.provider_model_picker_provider_choices()
                .into_iter()
                .fold(column![].spacing(1).width(Fill), |list, provider| {
                    let enabled_count = picker
                        .draft_enabled_model_ids_by_provider
                        .get(&provider.id)
                        .map(BTreeSet::len)
                        .unwrap_or_else(|| {
                            provider_config::effective_model_choices(
                                &provider.id,
                                &self.provider_configuration,
                            )
                            .len()
                        });
                    let trailing_badge: Element<'_, Message> = text(format!("[{}]", enabled_count))
                        .size(12)
                        .color(
                            self.app_theme
                                .extended_palette()
                                .background
                                .base
                                .text
                                .scale_alpha(0.56),
                        )
                        .into();
                    list.push(self.view_tree_navigation_button_row(
                        self.view_settings_navigation_icon(SettingsSection::ModelProviders, true),
                        provider.name.clone(),
                        None,
                        Some(trailing_badge),
                        picker.selected_provider_id == provider.id,
                        MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                        Message::ProviderModelPickerProviderSelected(provider.id),
                    ))
                }),
        )
        .height(320);
        let filtered_catalog = self.filtered_provider_model_picker_catalog();
        let catalog_list: Element<'_, Message> = if filtered_catalog.is_empty() {
            container(text("当前过滤条件下没有匹配模型。").size(13))
                .width(Fill)
                .height(260)
                .center(Fill)
                .into()
        } else {
            scrollable(
                filtered_catalog
                    .into_iter()
                    .fold(column![].spacing(10), |list, model| {
                        let model_id = model.id.clone();
                        let provider_id = picker.selected_provider_id.clone();
                        let checked = current_draft_enabled_ids.contains(&model_id);
                        let mut subtitle_parts = Vec::new();
                        if let Some(family) = model.family.as_deref() {
                            subtitle_parts.push(family.to_string());
                        }
                        if !model.input_modalities.is_empty() {
                            subtitle_parts.push(model.input_modalities.join(" / "));
                        } else {
                            subtitle_parts.push(String::from("text"));
                        }
                        if let Some(context_limit) = model.context_limit {
                            subtitle_parts.push(format!("上下文 {context_limit}"));
                        }
                        if model.default_enabled {
                            subtitle_parts.push(String::from("默认勾选"));
                        }
                        list.push(
                            container(
                                column![
                                    row![
                                        checkbox(checked)
                                            .label(model.name.clone())
                                            .on_toggle(move |enabled| {
                                                Message::ProviderModelPickerModelToggled {
                                                    provider_id: provider_id.clone(),
                                                    model_id: model_id.clone(),
                                                    enabled,
                                                }
                                            })
                                            .size(13)
                                            .width(Fill),
                                        button("编辑能力").style(button::secondary).on_press(
                                            Message::OpenProviderModelOverrideEditor {
                                                provider_id: picker.selected_provider_id.clone(),
                                                model_id: model.id.clone(),
                                            }
                                        ),
                                    ]
                                    .align_y(Center)
                                    .spacing(10),
                                    text(format!("{} · {}", model.id, subtitle_parts.join(" · ")))
                                        .size(11)
                                        .color(
                                            self.app_theme
                                                .extended_palette()
                                                .background
                                                .base
                                                .text
                                                .scale_alpha(0.56),
                                        ),
                                ]
                                .spacing(6),
                            )
                            .padding(10)
                            .style(container::bordered_box),
                        )
                    }),
            )
            .height(260)
            .into()
        };
        let modal_card = container(
            column![
                row![
                    text("配置启用模型").size(20),
                    space::horizontal(),
                    text(format!("当前草稿共勾选 {} 项", total_draft_enabled_count)).size(12),
                    close_button,
                ]
                .align_y(Center),
                text("左侧切换供应商，右侧编辑供应商内置完整模型目录。全选、取消等操作都只写入草稿，只有点击保存才会真正落盘。")
                    .size(12),
                row![
                    container(column![text("供应商").size(13), provider_list].spacing(8))
                        .width(FillPortion(2)),
                    container(
                        column![
                            row![
                                text("模型目录").size(13),
                                space::horizontal(),
                                text_input("输入模型名称或 ID 过滤", &picker.filter)
                                    .on_input(Message::ProviderModelPickerFilterChanged)
                                    .padding(8)
                                    .width(260),
                            ]
                            .align_y(Center),
                            row![
                                button("全选当前结果")
                                    .style(button::secondary)
                                    .on_press(Message::ProviderModelPickerSelectAllVisible),
                                button("取消当前结果")
                                    .style(button::secondary)
                                    .on_press(Message::ProviderModelPickerClearAllVisible),
                            ]
                            .spacing(10),
                            catalog_list,
                        ]
                        .spacing(10),
                    )
                    .width(FillPortion(5)),
                ]
                .spacing(12)
                .height(360),
                row![
                    text("取消会丢弃当前草稿，不会污染已保存结果。").size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderModelPicker),
                ]
                .align_y(Center),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(920)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderModelPicker);

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

    /// Renders the blocking per-model metadata editor used to correct one enabled model's capability declaration.
    /// 渲染用于修正单个已启用模型能力声明的阻塞式逐模型元数据编辑弹窗。
    pub(super) fn view_provider_model_override_editor_modal(&self) -> Option<Element<'_, Message>> {
        let editor = self.provider_model_override_editor.as_ref()?;
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
        .on_press(Message::CloseProviderModelOverrideEditor);
        let modality_toggles = column![
            checkbox(editor.supports_text)
                .label("文本输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("text"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_image)
                .label("图片输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("image"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_audio)
                .label("音频输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("audio"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_video)
                .label("视频输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("video"),
                        enabled,
                    }
                ),
            checkbox(editor.supports_pdf)
                .label("PDF / 文件输入")
                .on_toggle(
                    |enabled| Message::ProviderModelOverrideInputModalityToggled {
                        modality: String::from("pdf"),
                        enabled,
                    }
                ),
        ]
        .spacing(8);
        let modal_card = container(
            column![
                row![
                    text("编辑模型能力").size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                text(format!(
                    "{} · {}",
                    provider_config::provider_summary(&editor.provider_id),
                    editor.model_name
                ))
                .size(12),
                row![
                    checkbox(editor.reasoning)
                        .label("声明支持思考 / 推理")
                        .on_toggle(Message::ProviderModelOverrideReasoningChanged),
                    tooltip(
                        checkbox(editor.attachment)
                            .label("声明附件标记")
                            .on_toggle(Message::ProviderModelOverrideAttachmentChanged),
                        "用于记录目录级附件声明，主要服务于诊断摘要与元数据对齐；真正决定图片、音频、视频、PDF 能否直接进入模型输入的是下方“输入能力”开关。",
                        tooltip::Position::Bottom,
                    ),
                ]
                .spacing(18),
                container(column![text("输入能力").size(13), modality_toggles].spacing(8))
                    .padding(12)
                    .style(container::bordered_box),
                row![
                    column![
                        row![
                            text("上下文长度").size(12),
                            tooltip(
                                container(
                                    text("?").size(11).color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.56),
                                    ),
                                )
                                .padding([0, 4]),
                                "表示模型声明的总上下文窗口上限，通常覆盖系统提示词、历史消息、当前输入以及部分运行时预留空间。",
                                tooltip::Position::Bottom,
                            ),
                        ]
                        .spacing(4)
                        .align_y(Center),
                        text_input("例如 131072", &editor.context_limit_text)
                            .on_input(Message::ProviderModelOverrideContextLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                    column![
                        row![
                            text("输入长度").size(12),
                            tooltip(
                                container(
                                    text("?").size(11).color(
                                        self.app_theme
                                            .extended_palette()
                                            .background
                                            .base
                                            .text
                                            .scale_alpha(0.56),
                                    ),
                                )
                                .padding([0, 4]),
                                "表示宿主优先采用的输入预算上限，会直接影响上下文占用、预警与压缩判断；如果留空，则回退到“上下文长度”。它不一定等于“上下文长度 - 输出长度”，因为有些供应商会单独限制最大输入预算。",
                                tooltip::Position::Bottom,
                            ),
                        ]
                        .spacing(4)
                        .align_y(Center),
                        text_input("例如 65536", &editor.input_limit_text)
                            .on_input(Message::ProviderModelOverrideInputLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                    column![
                        text("输出长度").size(12),
                        text_input("例如 8192", &editor.output_limit_text)
                            .on_input(Message::ProviderModelOverrideOutputLimitChanged)
                            .padding(8)
                            .width(Fill),
                    ]
                    .spacing(6)
                    .width(Fill),
                ]
                .spacing(10),
                column![
                    text("思考模式附加配置（可选 JSON）").size(12),
                    container(
                        text_editor(&editor.reasoning_profile_buffer)
                            .height(140)
                            .padding(10)
                            .size(13)
                            .line_height(Pixels(18.0))
                            .font(theme::code_font())
                            .wrapping(iced::widget::text::Wrapping::None)
                            .on_action(Message::ProviderModelOverrideReasoningProfileAction),
                    )
                    .style(container::bordered_box)
                    .width(Fill),
                    text("支持多行 JSON；留空则不写入额外思考配置。")
                        .size(11)
                        .color(
                            self.app_theme
                                .extended_palette()
                                .background
                                .base
                                .text
                                .scale_alpha(0.56),
                        ),
                ]
                .spacing(6),
                row![
                    text("修改只会写入该模型的用户覆盖配置；点击保存前不会影响正式结果。").size(12),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveProviderModelOverrideEditor),
                ]
                .align_y(Center),
            ]
            .spacing(14),
        )
        .padding([22, 24])
        .width(760)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseProviderModelOverrideEditor);

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

    /// Renders the runtime selector card used by both the provider page and the runtime diagnostics page.
    /// 渲染供供应商页与运行诊断页复用的运行参数选择卡片。
    pub(super) fn view_runtime_selector_settings_card(&self) -> Element<'_, Message> {
        let temperature_control: Element<'_, Message> = if self.temperature_enabled {
            slider(0.0..=1.0, self.temperature, Message::TemperatureChanged)
                .step(0.05)
                .into()
        } else {
            text(self.t(I18nKey::SettingsTemperatureDisabledNotice))
                .size(12)
                .into()
        };

        container(
            column![
                text(self.t(I18nKey::SettingsRuntimeParametersTitle)).size(18),
                text(self.t(I18nKey::SettingsRuntimeParametersDescription)).size(12),
                text(self.t(I18nKey::CommonLanguageLabel)).size(12),
                pick_list(
                    self.language_manager.available_locales(),
                    Some(self.current_locale()),
                    Message::LocaleSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsLanguagePlaceholder)),
                text(self.t(I18nKey::SettingsSessionTranscriptTurnPageSizeLabel)).size(12),
                pick_list(
                    SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS,
                    Some(self.session_transcript_turn_page_size),
                    Message::SessionTranscriptTurnPageSizeSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsSessionTranscriptTurnPageSizePlaceholder)),
                text(match self.current_locale() {
                    AppLocale::ZhCn => "默认命令 Shell",
                    AppLocale::EnUs => "Default Command Shell",
                })
                .size(12),
                pick_list(
                    self.command_shell_picker_choices(),
                    self.selected_command_shell_picker_choice(),
                    Message::CommandShellPreferenceSelected
                )
                .width(Fill)
                .placeholder(match self.current_locale() {
                    AppLocale::ZhCn => "选择默认命令 Shell",
                    AppLocale::EnUs => "Choose a default command shell",
                }),
                text(self.command_shell_support_summary()).size(12),
                text(self.t(I18nKey::CommonThemeLabel)).size(12),
                pick_list(
                    Theme::ALL,
                    Some(self.app_theme.clone()),
                    Message::ThemeSelected
                )
                .width(Fill)
                .placeholder(self.t(I18nKey::SettingsThemePlaceholder)),
                checkbox(self.temperature_enabled)
                    .label(self.t(I18nKey::SettingsTemperatureToggle))
                    .on_toggle(Message::TemperatureEnabledToggled),
                text(if self.temperature_enabled {
                    self.tf(
                        I18nKey::SettingsTemperatureValueEnabled,
                        &[I18nArg::named(
                            "temperature",
                            format!("{:.2}", self.temperature),
                        )],
                    )
                } else {
                    self.t(I18nKey::SettingsTemperatureValueDefault)
                }),
                temperature_control,
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the engineering policy control card.
    /// 渲染工程策略控制卡片。
    pub(super) fn view_policy_controls_card(&self) -> Element<'_, Message> {
        container(
            column![
                text(self.t(I18nKey::SettingsPolicyTitle)).size(18),
                checkbox(self.generate_plan)
                    .label(self.t(I18nKey::SettingsPolicyGeneratePlan))
                    .on_toggle(Message::GeneratePlanToggled),
                checkbox(self.run_cargo_check)
                    .label(self.t(I18nKey::SettingsPolicyRunCargoCheck))
                    .on_toggle(Message::RunCargoCheckToggled),
                checkbox(self.persist_memory)
                    .label(self.t(I18nKey::SettingsPolicyPersistMemory))
                    .on_toggle(Message::PersistMemoryToggled),
                toggler(self.auto_approve)
                    .label(self.t(I18nKey::SettingsPolicyAutoApprove))
                    .on_toggle(Message::AutoApproveToggled),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the integration guide card shown in runtime diagnostics.
    /// 渲染显示在运行诊断页中的集成指引卡片。
    pub(super) fn view_integration_guide_card(&self) -> Element<'_, Message> {
        container(
            column![
                text(self.t(I18nKey::SettingsIntegrationGuideTitle)).size(18),
                self.view_workbench_markdown_body(
                    Some(&self.quick_guide_markdown_buffer),
                    self.localized_quick_guide_markdown(),
                    Message::QuickGuideMarkdownAction,
                ),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the activity-log card shown in runtime diagnostics.
    /// 渲染显示在运行诊断页中的活动日志卡片。
    pub(super) fn view_activity_log_card(&self) -> Element<'_, Message> {
        container(
            self.activities.iter().enumerate().fold(
                column![
                    text(self.t(I18nKey::SettingsActivityLogTitle)).size(18),
                    text(self.t(I18nKey::SettingsActivityLogHint)).size(12),
                ]
                .spacing(8),
                |list, (index, entry)| list.push(self.view_activity_card(index, entry)),
            ),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the runtime diagnostics page with execution status, policy controls, and engineering breadcrumbs.
    /// 渲染包含执行状态、策略控制与工程轨迹的运行诊断页面。
    pub(super) fn view_inspector(&self) -> Element<'_, Message> {
        column![
            self.view_runtime_selector_settings_card(),
            self.view_runtime_execution_card(),
            self.view_policy_controls_card(),
            self.view_integration_guide_card(),
            self.view_activity_log_card(),
        ]
        .spacing(14)
        .into()
    }

    /// Renders the execution runtime card that exposes active-agent selection and the latest preparation snapshot.
    /// 渲染执行运行时卡片，以暴露激活代理选择与最近一次执行准备快照。
    pub(super) fn view_runtime_execution_card(&self) -> Element<'_, Message> {
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

    /// Renders the MCP configuration page as a pure server list with compact edit and remove actions.
    /// 将 MCP 配置页渲染为纯服务器列表，并提供紧凑的编辑与移除动作。
    pub(super) fn view_mcp_configuration_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let server_list: Element<'_, Message> = if self
            .provider_configuration
            .mcp
            .servers
            .is_empty()
        {
            container(
                column![
                    text("还没有 MCP。").size(13),
                    text("点击右上角“新增 MCP”添加一个自定义服务器。").size(12),
                ]
                .spacing(6),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into()
        } else {
            scrollable(self.provider_configuration.mcp.servers.iter().fold(
                column![].spacing(10),
                |list, (server_id, entry)| {
                    let transport_label = match entry.transport {
                        PersistedMcpTransportKind::Stdio => "STDIO",
                        PersistedMcpTransportKind::Http => "流式 HTTP",
                        PersistedMcpTransportKind::Sse => "兼容 SSE",
                    };
                    let status_label = if entry.enabled {
                        "已启用"
                    } else {
                        "已停用"
                    };
                    let endpoint_summary = match entry.transport {
                        PersistedMcpTransportKind::Stdio => {
                            let argument_suffix = if entry.args.is_empty() {
                                String::new()
                            } else {
                                format!(" {}", entry.args.join(" "))
                            };
                            format!("{}{}", entry.command, argument_suffix)
                        }
                        PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => entry
                            .url
                            .clone()
                            .unwrap_or_else(|| String::from("未配置 URL")),
                    };
                    let metadata_summary = format!(
                        "{status_label} · {transport_label}{}",
                        if entry.oauth.is_some()
                            && !matches!(entry.transport, PersistedMcpTransportKind::Stdio)
                        {
                            " · OAuth"
                        } else {
                            ""
                        }
                    );
                    let edit_choice = McpServerChoice {
                        id: server_id.clone(),
                        label: entry.name.clone(),
                    };

                    list.push(
                        container(
                            row![
                                column![
                                    text(&entry.name).size(16),
                                    text(metadata_summary)
                                        .size(12)
                                        .color(palette.background.base.text.scale_alpha(0.62)),
                                    text(endpoint_summary)
                                        .size(12)
                                        .color(palette.background.base.text.scale_alpha(0.52)),
                                ]
                                .spacing(6)
                                .width(Fill),
                                row![
                                    button("编辑")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, false,
                                            )
                                        })
                                        .on_press(Message::McpServerSelected(edit_choice)),
                                    button("删除")
                                        .padding([4, 10])
                                        .style(|theme, status| {
                                            style::workbench_label_action_button_style(
                                                theme, status, true,
                                            )
                                        })
                                        .on_press(Message::RequestRemoveMcpServerConfiguration(
                                            server_id.clone(),
                                        ),),
                                ]
                                .spacing(10),
                            ]
                            .align_y(Center)
                            .spacing(12),
                        )
                        .padding(12)
                        .style(container::bordered_box),
                    )
                },
            ))
            .height(Fill)
            .width(Fill)
            .into()
        };

        container(server_list).width(Fill).height(Fill).into()
    }

    /// Renders the blocking MCP editor overlay used for creating or updating one custom MCP server entry.
    /// 渲染用于创建或更新单条自定义 MCP 服务器配置的阻塞式编辑遮罩层。
    pub(super) fn view_mcp_server_editor_modal(&self) -> Option<Element<'_, Message>> {
        if !self.mcp_server_editor_open {
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
        .on_press(Message::CloseMcpServerEditor);
        let transport_selector = row(mcp_transport_choices()
            .into_iter()
            .map(|transport| {
                let label = match transport {
                    PersistedMcpTransportKind::Stdio => "STDIO",
                    PersistedMcpTransportKind::Http => "流式 HTTP",
                    PersistedMcpTransportKind::Sse => "兼容 SSE",
                };
                button(container(text(label).size(13)).width(Fill).center_x(Fill))
                    .width(Fill)
                    .padding([10, 12])
                    .style(move |theme, status| {
                        style::workbench_transcript_mode_button_style(
                            theme,
                            status,
                            self.mcp_server_form.transport == transport,
                        )
                    })
                    .on_press(Message::McpServerTransportSelected(transport))
                    .into()
            })
            .collect::<Vec<Element<'_, Message>>>())
        .spacing(8)
        .width(Fill);
        let build_row_remove_button = |enabled: bool, on_press: Message| -> Element<'_, Message> {
            let button = button(
                container(lucide::trash().size(14).line_height(1.0))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(32)
            .height(32)
            .padding(0)
            .style(style::workbench_model_trigger_button_style);
            if enabled {
                button.on_press(on_press).into()
            } else {
                button.into()
            }
        };
        let transport_fields: Element<'_, Message> = match self.mcp_server_form.transport {
            PersistedMcpTransportKind::Stdio => {
                let allow_remove_args = self.mcp_server_form.args_rows.len() > 1;
                let argument_rows = self.mcp_server_form.args_rows.iter().enumerate().fold(
                    column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("参数", &row_state.value)
                                    .on_input(move |value| Message::McpServerArgRowChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(Fill),
                                build_row_remove_button(
                                    allow_remove_args,
                                    Message::RemoveMcpServerArgRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_env_rows = self.mcp_server_form.env_rows.len() > 1;
                let environment_rows = self.mcp_server_form.env_rows.iter().enumerate().fold(
                    column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("键", &row_state.key)
                                    .on_input(move |value| Message::McpServerEnvKeyChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(2)),
                                text_input("值", &row_state.value)
                                    .on_input(move |value| Message::McpServerEnvValueChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(3)),
                                build_row_remove_button(
                                    allow_remove_env_rows,
                                    Message::RemoveMcpServerEnvRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_passthrough_rows =
                    self.mcp_server_form.env_passthrough_rows.len() > 1;
                let passthrough_rows = self
                    .mcp_server_form
                    .env_passthrough_rows
                    .iter()
                    .enumerate()
                    .fold(column![].spacing(8), |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("环境变量名，例如 TEST_TOKEN", &row_state.value)
                                    .on_input(move |value| Message::McpServerEnvPassthroughChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(Fill),
                                build_row_remove_button(
                                    allow_remove_passthrough_rows,
                                    Message::RemoveMcpServerEnvPassthroughRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    });

                column![
                    column![
                        text("启动命令").size(13),
                        text_input(
                            "例如 openai-dev-mcp serve-sqlite",
                            &self.mcp_server_form.command
                        )
                        .on_input(Message::McpServerCommandChanged)
                        .padding(10)
                        .width(Fill),
                    ]
                    .spacing(6),
                    container(
                        column![
                            text("参数").size(13),
                            argument_rows,
                            button("+ 添加参数")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerArgRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        column![
                            text("环境变量").size(13),
                            environment_rows,
                            button("+ 添加环境变量")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerEnvRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        column![
                            text("环境变量传递").size(13),
                            passthrough_rows,
                            button("+ 添加变量")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerEnvPassthroughRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    column![
                        text("工作目录").size(13),
                        text_input(DEFAULT_MCP_WORKING_DIRECTORY, &self.mcp_server_form.cwd)
                            .on_input(Message::McpServerCwdChanged)
                            .padding(10)
                            .width(Fill),
                    ]
                    .spacing(6),
                ]
                .spacing(14)
                .into()
            }
            PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
                let endpoint_label = if matches!(
                    self.mcp_server_form.transport,
                    PersistedMcpTransportKind::Http
                ) {
                    "接口地址"
                } else {
                    "SSE 地址"
                };
                let mut start_oauth_button = button(if self.mcp_oauth_flow_in_flight {
                    "OAuth 进行中"
                } else {
                    "发起 OAuth"
                })
                .style(button::secondary);
                if self.selected_mcp_server_id.is_some() && !self.mcp_oauth_flow_in_flight {
                    start_oauth_button =
                        start_oauth_button.on_press(Message::StartSelectedMcpServerOAuth);
                }
                let mut complete_oauth_button = button("完成授权").style(button::secondary);
                if !self.mcp_oauth_callback_input.trim().is_empty() {
                    complete_oauth_button = complete_oauth_button
                        .on_press(Message::CompleteSelectedMcpServerOAuthFromInput);
                }
                let mut clear_oauth_button = button("清理凭据").style(button::secondary);
                if self.selected_mcp_server_id.is_some() {
                    clear_oauth_button = clear_oauth_button
                        .on_press(Message::RemoveSelectedMcpServerOAuthAuthorization);
                }
                let authorization_url_summary: Element<'_, Message> =
                    if self.mcp_oauth_authorization_url.trim().is_empty() {
                        text("最近一次授权链接：尚未生成").size(12).into()
                    } else {
                        text(format!(
                            "最近一次授权链接：{}",
                            self.mcp_oauth_authorization_url
                        ))
                        .size(12)
                        .into()
                    };
                let allow_remove_headers = self.mcp_server_form.header_rows.len() > 1;
                let header_rows = self.mcp_server_form.header_rows.iter().enumerate().fold(
                    column![].spacing(8),
                    |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("键", &row_state.key)
                                    .on_input(move |value| Message::McpServerHeaderKeyChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(2)),
                                text_input("值", &row_state.value)
                                    .on_input(move |value| Message::McpServerHeaderValueChanged(
                                        index, value
                                    ))
                                    .padding(10)
                                    .width(FillPortion(3)),
                                build_row_remove_button(
                                    allow_remove_headers,
                                    Message::RemoveMcpServerHeaderRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    },
                );
                let allow_remove_env_headers = self.mcp_server_form.header_from_env_rows.len() > 1;
                let header_from_env_rows = self
                    .mcp_server_form
                    .header_from_env_rows
                    .iter()
                    .enumerate()
                    .fold(column![].spacing(8), |list, (index, row_state)| {
                        list.push(
                            row![
                                text_input("Header 键", &row_state.key)
                                    .on_input(move |value| {
                                        Message::McpServerHeaderFromEnvKeyChanged(index, value)
                                    })
                                    .padding(10)
                                    .width(FillPortion(2)),
                                text_input("环境变量名", &row_state.value)
                                    .on_input(move |value| {
                                        Message::McpServerHeaderFromEnvValueChanged(index, value)
                                    })
                                    .padding(10)
                                    .width(FillPortion(3)),
                                build_row_remove_button(
                                    allow_remove_env_headers,
                                    Message::RemoveMcpServerHeaderFromEnvRow(index),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Center),
                        )
                    });

                column![
                    column![
                        text(endpoint_label).size(13),
                        text_input("例如 https://example.com/mcp", &self.mcp_server_form.url,)
                            .on_input(Message::McpServerUrlChanged)
                            .padding(10)
                            .width(Fill),
                    ]
                    .spacing(6),
                    column![
                        text("Bearer Token").size(13),
                        text_input(
                            "可选，仅填写 Token 内容",
                            &self.mcp_server_form.bearer_token,
                        )
                        .on_input(Message::McpServerBearerTokenChanged)
                        .padding(10)
                        .secure(true)
                        .width(Fill),
                    ]
                    .spacing(6),
                    container(
                        column![
                            text("请求头").size(13),
                            header_rows,
                            button("+ 添加请求头")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerHeaderRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        column![
                            text("来自环境变量的请求头").size(13),
                            header_from_env_rows,
                            button("+ 添加环境变量请求头")
                                .style(button::secondary)
                                .on_press(Message::AddMcpServerHeaderFromEnvRow),
                        ]
                        .spacing(8),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                    container(
                        column![
                            text("OAuth（可选）").size(14),
                            text(format!("当前认证状态：{}", self.mcp_oauth_status)).size(12),
                            column![
                                text("Client ID").size(13),
                                text_input(
                                    "留空则尝试动态注册",
                                    &self.mcp_server_form.oauth_client_id,
                                )
                                .on_input(Message::McpServerOAuthClientIdChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            column![
                                text("Client Secret").size(13),
                                text_input("可选", &self.mcp_server_form.oauth_client_secret,)
                                    .on_input(Message::McpServerOAuthClientSecretChanged)
                                    .padding(10)
                                    .secure(true)
                                    .width(Fill),
                            ]
                            .spacing(6),
                            column![
                                text("Scopes").size(13),
                                text_input(
                                    "使用分号、逗号或空格分隔",
                                    &self.mcp_server_form.oauth_scopes_text,
                                )
                                .on_input(Message::McpServerOAuthScopesChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            column![
                                text("回调地址").size(13),
                                text_input(
                                    "建议填写 localhost 回调地址",
                                    &self.mcp_server_form.oauth_redirect_uri,
                                )
                                .on_input(Message::McpServerOAuthRedirectUriChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            authorization_url_summary,
                            column![
                                text("手动回调").size(13),
                                text_input(
                                    "浏览器最终跳转 URL、/callback?... 路径或 query string",
                                    &self.mcp_oauth_callback_input,
                                )
                                .on_input(Message::McpServerOAuthCallbackInputChanged)
                                .padding(10)
                                .width(Fill),
                            ]
                            .spacing(6),
                            row![
                                start_oauth_button,
                                complete_oauth_button,
                                clear_oauth_button,
                            ]
                            .spacing(10),
                        ]
                        .spacing(12),
                    )
                    .padding(14)
                    .style(container::bordered_box)
                    .width(Fill),
                ]
                .spacing(14)
                .into()
            }
        };
        let notice_text: Element<'_, Message> = if self.mcp_notice.trim().is_empty() {
            container(space()).height(0).into()
        } else {
            text(&self.mcp_notice)
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.68))
                .into()
        };
        let modal_body = scrollable(
            column![
                column![
                    text("名称").size(13),
                    text_input("MCP server name", &self.mcp_server_form.name)
                        .on_input(Message::McpServerNameChanged)
                        .padding(10)
                        .width(Fill),
                ]
                .spacing(6),
                column![text("连接方式").size(13), transport_selector].spacing(6),
                transport_fields,
                notice_text,
            ]
            .spacing(14),
        )
        .height(560)
        .spacing(8);
        let modal_card = container(
            column![
                row![
                    text(if self.selected_mcp_server_id.is_some() {
                        "编辑自定义 MCP"
                    } else {
                        "连接至自定义 MCP"
                    })
                    .size(20),
                    space::horizontal(),
                    close_button,
                ]
                .align_y(Center),
                modal_body,
                row![
                    checkbox(self.mcp_server_form.enabled)
                        .label("启用该服务器")
                        .on_toggle(Message::McpServerEnabledToggled),
                    space::horizontal(),
                    button("保存")
                        .style(button::primary)
                        .on_press(Message::SaveMcpServerConfiguration),
                ]
                .align_y(Center),
            ]
            .spacing(16),
        )
        .padding([22, 24])
        .width(820)
        .style(style::startup_overlay_card_style);
        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::CloseMcpServerEditor);

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

    /// Renders the project-scoped historical attachment cleanup card used inside the project settings modal.
    /// 渲染项目设置弹窗内部使用的项目级历史附件清理卡片。
    pub(super) fn view_project_attachment_cleanup_card<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let cleanup_busy = self.project_attachment_cleanup_busy();
        let preview_button = {
            let preview_button = button(if self.project_attachment_cleanup_preview_in_flight {
                "预览中..."
            } else {
                "预览清理范围"
            })
            .style(button::secondary);
            if cleanup_busy {
                preview_button
            } else {
                preview_button.on_press(Message::PreviewProjectAttachmentCleanup)
            }
        };
        let cleanup_preview_can_execute = self
            .project_attachment_cleanup_preview
            .as_ref()
            .is_some_and(|preview| {
                preview.matched_asset_count > 0
                    && preview.cutoff_date_text
                        == self.project_attachment_cleanup_cutoff_date.to_string()
            });
        let execute_button = {
            let execute_button = button(if self.project_attachment_cleanup_execution_in_flight {
                "清理中..."
            } else {
                "执行清理"
            })
            .style(button::primary);
            if cleanup_preview_can_execute && !cleanup_busy {
                execute_button.on_press(Message::ExecuteProjectAttachmentCleanup)
            } else {
                execute_button
            }
        };
        let date_button = button(
            container(
                row![
                    text(format!(
                        "截止日期：{}",
                        self.project_attachment_cleanup_cutoff_date
                    ))
                    .size(13),
                    space::horizontal(),
                    codicon::calendar().size(14).line_height(1.0),
                ]
                .align_y(Center)
                .spacing(8),
            )
            .width(Fill),
        )
        .padding([8, 12])
        .style(button::secondary);
        let date_button = if cleanup_busy {
            date_button
        } else {
            date_button.on_press(Message::OpenProjectAttachmentCleanupDatePicker)
        };
        let date_picker_control: Element<'a, Message> = date_picker(
            self.project_attachment_cleanup_date_picker_open,
            self.project_attachment_cleanup_cutoff_date,
            date_button,
            Message::CancelProjectAttachmentCleanupDatePicker,
            Message::ProjectAttachmentCleanupDateSelected,
        )
        .into();

        let preview_summary: Element<'_, Message> =
            if let Some(preview) = self.project_attachment_cleanup_preview.as_ref() {
                let kind_summary = if preview.per_kind_counts.is_empty() {
                    String::from("未匹配任何附件类型")
                } else {
                    preview
                        .per_kind_counts
                        .iter()
                        .map(|(kind, count)| format!("{kind}:{count}"))
                        .collect::<Vec<_>>()
                        .join(" / ")
                };
                column![
                    text("最近一次预览").size(13),
                    text(self.format_attachment_cleanup_preview_summary(preview)).size(12),
                    text(format!("类型分布：{kind_summary}")).size(12),
                ]
                .spacing(6)
                .into()
            } else {
                text("尚未生成清理预览。").size(12).into()
            };

        let report_summary: Element<'_, Message> =
            if let Some(report) = self.project_attachment_cleanup_last_report.as_ref() {
                column![
                    text("最近一次执行").size(13),
                    text(self.format_attachment_cleanup_report_summary(report)).size(12),
                ]
                .spacing(6)
                .into()
            } else {
                text("尚未执行历史附件缓存清理。").size(12).into()
            };
        let attachment_root =
            crate::attachment_assets::resolve_managed_attachment_root(&project.path);

        container(
            column![
                row![
                    text("项目附件缓存清理").size(18),
                    space::horizontal(),
                    text("仅清理当前项目").size(12),
                ]
                .align_y(Center),
                text(
                    "选择截止日期后可先预览当前项目将被清理的托管附件范围。清理完成后，旧会话中的失效附件会自动撤销回放，不再阻断后续对话。",
                )
                .size(12),
                text(format!("托管目录：{}", attachment_root.display())).size(12),
                date_picker_control,
                column![
                    row![
                        {
                            let image_checkbox = checkbox(self.project_attachment_cleanup_selection.image)
                                .label("图片");
                            if cleanup_busy {
                                image_checkbox
                            } else {
                                image_checkbox.on_toggle(|enabled| Message::ProjectAttachmentCleanupKindToggled(
                                    ConversationAttachmentKind::Image,
                                    enabled,
                                ))
                            }
                        },
                        {
                            let audio_checkbox = checkbox(self.project_attachment_cleanup_selection.audio)
                                .label("音频");
                            if cleanup_busy {
                                audio_checkbox
                            } else {
                                audio_checkbox.on_toggle(|enabled| Message::ProjectAttachmentCleanupKindToggled(
                                    ConversationAttachmentKind::Audio,
                                    enabled,
                                ))
                            }
                        },
                    ]
                    .spacing(12),
                    row![
                        {
                            let video_checkbox = checkbox(self.project_attachment_cleanup_selection.video)
                                .label("视频");
                            if cleanup_busy {
                                video_checkbox
                            } else {
                                video_checkbox.on_toggle(|enabled| Message::ProjectAttachmentCleanupKindToggled(
                                    ConversationAttachmentKind::Video,
                                    enabled,
                                ))
                            }
                        },
                        {
                            let pdf_checkbox = checkbox(self.project_attachment_cleanup_selection.pdf)
                                .label("PDF");
                            if cleanup_busy {
                                pdf_checkbox
                            } else {
                                pdf_checkbox.on_toggle(|enabled| Message::ProjectAttachmentCleanupKindToggled(
                                    ConversationAttachmentKind::Pdf,
                                    enabled,
                                ))
                            }
                        },
                    ]
                    .spacing(12),
                ]
                .spacing(8),
                row![preview_button, execute_button].spacing(10),
                preview_summary,
                report_summary,
                text(&self.project_attachment_cleanup_notice).size(12),
            ]
            .spacing(12),
        )
        .padding(16)
        .style(container::bordered_box)
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
    use super::workspace_history_search_preview_segments;

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
}

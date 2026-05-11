//! View composition methods extracted from the main workbench module.
//! 从主工作台模块中抽取的界面组合方法。

use super::helpers::*;
use super::*;
use iced::widget::column;

impl VulcanWorkbench {
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

        let composed_surface: Element<'_, Message> =
            if let Some(overlay) = self.view_workbench_model_picker_modal() {
                stack([main_surface, overlay]).into()
            } else {
                main_surface
            };
        let composed_surface: Element<'_, Message> =
            if let Some(overlay) = self.view_startup_overlay() {
                stack([composed_surface, overlay]).into()
            } else {
                composed_surface
            };

        self.wrap_with_window_resize_handles(window_id, composed_surface)
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
                container(text("没有匹配的模型。").size(14))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill),
            )
            .width(Fill)
            .height(WORKBENCH_MODEL_PICKER_LIST_HEIGHT)
            .into()
        } else {
            scrollable(
                column(
                    filtered_choices
                        .into_iter()
                        .enumerate()
                        .map(|(index, choice)| {
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
                            .padding([8, 12])
                            .style(move |theme, status| {
                                style::workbench_model_picker_option_button_style(
                                    theme,
                                    status,
                                    is_selected,
                                )
                            })
                            .on_press(Message::WorkbenchModelSelected(choice.clone()))
                            .into()
                        }),
                )
                .spacing(6),
            )
            .height(WORKBENCH_MODEL_PICKER_LIST_HEIGHT)
            .into()
        };
        let modal_card = container(
            column![
                row![text("选择模型").size(20), space::horizontal(), close_button,].align_y(Center),
                text_input(
                    "输入模型或供应商进行过滤",
                    &self.workbench_model_picker_filter
                )
                .id(WORKBENCH_MODEL_PICKER_FILTER_INPUT_ID)
                .on_input(Message::WorkbenchModelPickerFilterChanged)
                .on_submit(Message::ConfirmWorkbenchModelPickerSelection)
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
            stack([
                container(backdrop)
                    .width(Fill)
                    .height(Fill)
                    .style(style::startup_overlay_backdrop_style)
                    .into(),
                container(modal_card)
                    .width(Fill)
                    .height(Fill)
                    .center(Fill)
                    .into(),
            ])
            .width(Fill)
            .height(Fill)
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

    /// Builds the tree-shaped workspace and session outline shown inside the main task drawer.
    /// 构建展示在主任务抽屉中的树形工作空间与会话大纲。
    pub(super) fn view_main_task_session_tree(&self) -> Element<'_, Message> {
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

    /// Builds the inline destructive second-confirmation card shown before one archived item is permanently deleted.
    /// 构建在某个归档项被永久删除前展示的内联破坏性二次确认卡片。
    pub(super) fn view_archived_delete_confirmation_card(
        &self,
        confirmation: &ArchivedDeleteConfirmationState,
    ) -> Element<'_, Message> {
        let subject_label = match &confirmation.target {
            ArchivedDeleteTarget::Project(_) => "归档项目",
            ArchivedDeleteTarget::Session(_) => "归档会话",
        };

        container(
            column![
                text(format!(
                    "将彻底删除{} `{}`，此操作不可恢复。",
                    subject_label, confirmation.title
                ))
                .size(13),
                row![
                    button("取消")
                        .padding([8, 12])
                        .style(button::secondary)
                        .on_press(Message::CancelPermanentDeleteArchivedItem),
                    button("确认删除")
                        .padding([8, 12])
                        .style(button::danger)
                        .on_press(Message::ConfirmPermanentDeleteArchivedItem),
                ]
                .spacing(8)
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(12)
        .width(Fill)
        .style(container::rounded_box)
        .into()
    }

    /// Builds one compact section label used inside the project tree.
    /// 构建展示在项目树内部的紧凑分区标签。
    pub(super) fn view_main_task_group_label(
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
    pub(super) fn view_main_task_project_drop_indicator(
        &self,
        is_visible: bool,
    ) -> Element<'_, Message> {
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
    pub(super) fn view_main_task_project_section_tail(
        &self,
        section_is_pinned: bool,
    ) -> Element<'_, Message> {
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
    pub(super) fn view_main_task_project_tree_row<'a>(
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
    pub(super) fn view_main_task_project_row<'a>(
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
    pub(super) fn view_main_task_project_children<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let has_visible_sessions = !project.session_ids.is_empty();

        if !has_visible_sessions {
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

        let content = project.session_ids.iter().fold(
            column![].spacing(4).width(Fill),
            |column, session_id| {
                if let Some(session) = self.session_by_id(*session_id) {
                    column.push(self.view_main_task_session_tree_row(session))
                } else {
                    column
                }
            },
        );

        content.into()
    }

    /// Builds one tree row shown under a project node inside the main task drawer.
    /// 构建显示在主任务抽屉某个项目节点下方的单条会话树节点。
    pub(super) fn view_main_task_session_tree_row<'a>(
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
    pub(super) fn view_main_task_project_menu<'a>(
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
    pub(super) fn view_main_task_session_menu<'a>(
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

    /// Builds one archived project row that can be restored back into the visible task tree.
    /// 构建一条可恢复回可见任务树的归档项目行。
    pub(super) fn view_archived_task_project_row<'a>(
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
    pub(super) fn view_archived_task_session_tree_row<'a>(
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
    pub(super) fn view_archived_task_project_menu<'a>(
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
                    "彻底删除",
                    Message::RequestPermanentDeleteArchivedTaskProject(project.id.clone()),
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
    pub(super) fn view_archived_task_session_menu<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let menu_width = 200.0;

        container(
            column![
                Self::view_main_task_tree_menu_button(
                    "恢复",
                    Message::RestoreTaskSession(session.id),
                ),
                Self::view_main_task_tree_menu_button(
                    "彻底删除",
                    Message::RequestPermanentDeleteArchivedTaskSession(session.id),
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
    pub(super) fn view_main_task_session_status_icon(
        status: SessionState,
    ) -> Element<'static, Message> {
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
    pub(super) fn view_main_task_tree_icon_action_button(
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
    pub(super) fn view_main_task_tree_menu_button(
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
    pub(super) fn has_active_run(&self) -> bool {
        self.active_run_id.is_some()
    }

    /// Clears the tracked active-run state after one AI call stops or finishes.
    /// 在一次 AI 调用停止或结束后清理已跟踪的活动运行状态。
    pub(super) fn clear_active_run_tracking(&mut self) {
        self.active_run_id = None;
        self.active_run_session_id = None;
        self.active_run_handle = None;
    }

    /// Returns whether the current composer buffer contains any submittable user instruction text.
    /// 返回当前提示词缓冲区是否包含任何可提交的用户指令文本。
    pub(super) fn composer_has_submittable_content(&self) -> bool {
        composer_text_has_submittable_content(&self.composer_buffer.text())
    }

    /// Builds the compact send-or-stop button shown at the right side of prompt inputs.
    /// 构建显示在提示词输入框右侧的紧凑发送或停止按钮。
    pub(super) fn view_run_trigger_button(&self) -> Element<'_, Message> {
        let is_running = self.has_active_run();
        let can_submit = self.composer_has_submittable_content();
        let tooltip_text = if is_running {
            "停止当前 AI 调用"
        } else if can_submit {
            "发送当前任务指令"
        } else {
            "请输入任务指令后再发送"
        };
        let trigger_message = if is_running {
            Message::StopRun
        } else {
            Message::StartRun
        };
        let icon = if is_running {
            codicon::debug_stop()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0)
        } else {
            lucide::send()
                .size(COMPOSER_TRIGGER_BUTTON_ICON_SIZE)
                .line_height(1.0)
        };
        let icon_surface: Element<'_, Message> = container(icon)
            .width(COMPOSER_TRIGGER_BUTTON_SIZE)
            .height(COMPOSER_TRIGGER_BUTTON_SIZE)
            .center(Fill)
            .into();

        let button = button(icon_surface).padding(0).style(move |theme, status| {
            style::workbench_composer_trigger_button_style(theme, status, is_running)
        });
        let button = if is_running || can_submit {
            button.on_press(trigger_message)
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
            .unwrap_or_default();
        let has_choices = !self.configured_workbench_model_choices().is_empty();
        let provider_line: Element<'_, Message> = if provider_label.is_empty() {
            container(space::vertical().height(11))
                .height(11)
                .width(Fill)
                .into()
        } else {
            container(
                text(provider_label)
                    .size(11)
                    .color(palette.background.base.text.scale_alpha(0.56))
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .height(11)
            .width(Fill)
            .clip(true)
            .center_x(Fill)
            .into()
        };
        let text_row = row![
            container(
                column![
                    container(
                        text(model_label)
                            .size(13)
                            .color(if has_selected_choice {
                                iced::Color::WHITE
                            } else {
                                palette.background.base.text.scale_alpha(0.56)
                            })
                            .wrapping(iced::advanced::text::Wrapping::None),
                    )
                    .height(14)
                    .width(Fill)
                    .clip(true)
                    .center_x(Fill),
                    provider_line,
                ]
                .spacing(2)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(COMPOSER_CONTROL_ROW_HEIGHT)
            .center_x(Fill)
            .center_y(Fill),
            codicon::chevron_down()
                .size(14)
                .line_height(1.0)
                .color(palette.background.base.text.scale_alpha(0.48)),
        ]
        .spacing(10)
        .align_y(Center);
        let trigger = button(container(text_row).width(Fill).center_y(Fill))
            .width(WORKBENCH_MODEL_SELECTOR_WIDTH)
            .height(COMPOSER_CONTROL_ROW_HEIGHT)
            .padding([0, 12])
            .style(style::workbench_model_trigger_button_style);

        if has_choices {
            trigger.on_press(Message::OpenWorkbenchModelPicker).into()
        } else {
            trigger.into()
        }
    }

    /// Builds the compact reasoning-effort selector shown before the trailing send button.
    /// 构建显示在尾部发送按钮之前的紧凑推理级别选择器。
    pub(super) fn view_workbench_reasoning_effort_selector(&self) -> Element<'_, Message> {
        pick_list(
            WorkbenchReasoningEffort::ALL,
            Some(self.workbench_reasoning_effort),
            Message::WorkbenchReasoningEffortSelected,
        )
        .width(WORKBENCH_REASONING_SELECTOR_WIDTH)
        .placeholder("级别")
        .padding([8, 10])
        .text_size(13)
        .line_height(iced::widget::text::LineHeight::Absolute(Pixels(20.0)))
        .style(style::workbench_compact_pick_list_style)
        .menu_style(style::workbench_compact_pick_list_menu_style)
        .into()
    }

    /// Returns the current visible line count of the main prompt composer.
    /// 返回主提示词编辑器当前可见的逻辑行数量。
    pub(super) fn composer_line_count(&self) -> usize {
        composer_visible_line_count(&self.composer_buffer)
    }

    /// Returns the current dynamic height of the main prompt composer editor.
    /// 返回主提示词编辑器当前的动态高度。
    pub(super) fn composer_editor_height(&self) -> f32 {
        let visible_lines = self.composer_line_count().min(10) as f32;

        if visible_lines <= 1.0 {
            COMPOSER_SINGLE_LINE_HEIGHT
        } else {
            COMPOSER_EDITOR_VERTICAL_INSET + visible_lines * COMPOSER_EDITOR_LINE_HEIGHT
        }
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

    /// Returns the floating composer footprint currently covering the bottom of the transcript viewport.
    /// 返回当前覆盖在转录视口底部的浮动提示词区域占用高度。
    pub(super) fn floating_composer_footprint_height(&self) -> f32 {
        let visible_editor_height = if self.composer_uses_expanded_surface() {
            self.composer_editor_height()
        } else {
            COMPOSER_SINGLE_LINE_HEIGHT
        };

        visible_editor_height
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
        let logical_line_count = self.composer_line_count();
        let visible_height = self.composer_editor_height();
        let can_submit = self.composer_has_submittable_content();
        let full_content_height = if logical_line_count <= 1 {
            visible_height
        } else {
            COMPOSER_EDITOR_VERTICAL_INSET + logical_line_count as f32 * COMPOSER_EDITOR_LINE_HEIGHT
        };

        let editor = text_editor(&self.composer_buffer)
            .id(COMPOSER_EDITOR_ID)
            .height(full_content_height)
            .size(COMPOSER_EDITOR_TEXT_SIZE)
            .line_height(iced::widget::text::LineHeight::Absolute(Pixels(
                COMPOSER_EDITOR_LINE_HEIGHT,
            )))
            .padding([10, 12])
            .placeholder("输入当前会话要执行的任务指令")
            .style(style::workbench_composer_editor_style)
            .key_binding(move |key_press| match key_press.key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Enter)
                    if !key_press.modifiers.shift() =>
                {
                    if can_submit {
                        Some(text_editor::Binding::Custom(Message::StartRun))
                    } else {
                        Some(text_editor::Binding::Sequence(Vec::new()))
                    }
                }
                _ => text_editor::Binding::from_key_press(key_press),
            })
            .on_action(Message::ComposerEditorAction);

        if logical_line_count > 10 {
            scrollable(container(editor).width(Fill))
                .id(COMPOSER_EDITOR_SCROLLABLE_ID)
                .height(visible_height)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(8)
                        .scroller_width(8)
                        .spacing(6),
                ))
                .on_scroll(Message::ComposerEditorScrolled)
                .into()
        } else {
            container(editor).width(Fill).into()
        }
    }

    /// Builds the compact placeholder slot used by the fixed composer panel row.
    /// 构建固定提示词面板行内使用的紧凑占位输入槽。
    pub(super) fn view_compact_composer_editor(&self) -> Element<'_, Message> {
        if self.composer_uses_expanded_surface() {
            container(space::vertical().height(COMPOSER_SINGLE_LINE_HEIGHT))
                .width(Fill)
                .into()
        } else {
            self.view_composer_editor_surface()
        }
    }

    /// Builds the expanded upward-growing composer overlay anchored above the fixed composer panel.
    /// 构建锚定在固定提示词面板上方的向上展开编辑器遮罩。
    pub(super) fn view_composer_expanded_overlay(&self) -> Option<Element<'_, Message>> {
        if !self.composer_uses_expanded_surface() {
            return None;
        }

        Some(
            container(self.view_composer_editor_surface())
                .width(self.composer_input_surface_width())
                .into(),
        )
    }

    /// Builds the compact transcript surface-mode switcher shown at the trailing end of the session-status row.
    /// 构建显示在会话状态行末端的紧凑转录表面模式切换器。
    pub(super) fn view_transcript_surface_mode_switcher(&self) -> Element<'_, Message> {
        let has_selected_session = self.selected_session().is_some();
        let is_detail_mode =
            self.agent_transcript_surface_mode == AgentTranscriptSurfaceMode::DetailCards;
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

        row![detail_mode_button, raw_mode_button]
            .spacing(8)
            .align_y(Center)
            .into()
    }

    /// Builds the central canvas displayed beside the task drawer in the main workbench panel.
    /// 构建显示在主工作台任务抽屉右侧的中央画布区域。
    pub(super) fn view_main_canvas(&self) -> Element<'_, Message> {
        let run_phase = self.selected_session_run_phase();
        let session_title = self
            .selected_session()
            .map(|session| session.title.as_str())
            .unwrap_or("未选择会话");
        let transcript_stream = self.view_main_canvas_transcript_stream();
        let transcript_surface_mode_switcher = self.view_transcript_surface_mode_switcher();
        let composer_overlay = self.view_composer_expanded_overlay();
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
        ]
        .spacing(COMPOSER_PANEL_CONTROL_GAP)
        .align_y(Center);
        let composer_secondary_controls = row![
            self.view_workbench_reasoning_effort_selector(),
            self.view_run_trigger_button(),
        ]
        .spacing(COMPOSER_PANEL_CONTROL_GAP)
        .align_y(Center);
        let composer_controls_row = row![
            composer_primary_controls,
            space::horizontal(),
            composer_secondary_controls,
        ]
        .spacing(COMPOSER_PANEL_CONTROL_GAP)
        .align_y(Center)
        .height(COMPOSER_CONTROL_ROW_HEIGHT)
        .width(Fill);
        let composer_panel = container(
            column![self.view_compact_composer_editor(), composer_controls_row,]
                .spacing(COMPOSER_PANEL_CONTROL_GAP),
        )
        .padding(COMPOSER_PANEL_EDGE_PADDING)
        .width(self.composer_panel_width())
        .style(style::workbench_composer_panel_style);
        let title_section = column![
            row![
                text(session_title).size(26),
                space::horizontal(),
                row![
                    transcript_surface_mode_switcher,
                    container(text(run_phase.label()).size(12))
                        .padding([4, 10])
                        .style(move |theme| style::workbench_run_phase_chip_style(
                            theme, run_phase
                        )),
                ]
                .spacing(10)
                .align_y(Center),
            ]
            .align_y(Center),
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

        if let Some(overlay) = composer_overlay {
            layered_surfaces.push(
                container(
                    column![
                        space::vertical().height(Fill),
                        container(overlay)
                            .width(self.composer_input_surface_width())
                            .center_x(Fill),
                        space::vertical().height(COMPOSER_OVERLAY_BOTTOM_OFFSET),
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

    /// Builds the transcript content surface shown in the center of the main workbench canvas.
    /// 构建展示在主工作台中央画布中的会话内容表面。
    pub(super) fn view_main_canvas_transcript_stream(&self) -> Element<'_, Message> {
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
                    text("输入任务并开始执行后，当前会话的输出会显示在这里。").size(13),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Center),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill)
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

        if let Some(overlay) = self.view_selected_session_jump_to_bottom_overlay(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_thinking_modal(session_id) {
            layered_surfaces.push(overlay);
        }

        if let Some(overlay) = self.view_selected_session_tool_modal(session_id) {
            layered_surfaces.push(overlay);
        }

        stack(layered_surfaces).into()
    }

    /// Builds the floating jump-to-bottom affordance shown after the user scrolls away from the newest transcript tail.
    /// 构建当用户滚离最新转录尾部后显示的浮动回底按钮。
    pub(super) fn view_selected_session_jump_to_bottom_overlay(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        if self.session_agent_view_should_follow_tail(session_id)
            || !self.session_agent_view_viewports.contains_key(&session_id)
        {
            return None;
        }

        let button = tooltip(
            button(
                container(codicon::chevron_down().size(18).line_height(1.0))
                    .width(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
                    .height(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
                    .center(Fill),
            )
            .padding(0)
            .width(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
            .height(TRANSCRIPT_JUMP_TO_BOTTOM_BUTTON_SIZE)
            .style(style::transcript_jump_to_bottom_button_style)
            .on_press(Message::JumpSelectedSessionTranscriptToBottom),
            "回到底部",
            tooltip::Position::Top,
        )
        .style(container::rounded_box);

        Some(
            container(
                column![
                    space::vertical().height(Fill),
                    row![
                        space::horizontal().width(Fill),
                        button,
                        space::horizontal().width(Fill)
                    ]
                    .width(Fill)
                    .align_y(Center),
                ]
                .width(Fill)
                .height(Fill),
            )
            .padding(iced::Padding {
                top: 14.0,
                right: 14.0,
                bottom: self.floating_composer_footprint_height()
                    + f32::from(TRANSCRIPT_JUMP_TO_BOTTOM_OVERLAY_BOTTOM_INSET),
                left: 14.0,
            })
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }

    /// Builds the unified rendered transcript stream used for per-block inspection during the transition period.
    /// 构建过渡期用于逐块检查的统一渲染转录流。
    pub(super) fn view_main_canvas_segmented_transcript_stream(
        &self,
        session_id: SessionId,
    ) -> Element<'_, Message> {
        let Some(document) = self.rendered_session_agent_view_document(session_id) else {
            return container(text("当前没有可渲染的统一输出块。").size(14))
                .padding(14)
                .height(Fill)
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
                    SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                        Message::OpenSelectedSessionThinkingModal(block_id)
                    }
                    SelectableTranscriptInteraction::OpenToolModal(block_id) => {
                        Message::OpenSelectedSessionToolModal(block_id)
                    }
                    SelectableTranscriptInteraction::ToggleBlockInline(_) => Message::Noop,
                },
            ),
            space::vertical().height(self.transcript_bottom_spacer_height()),
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
        .into()
    }

    /// Builds one reusable selectable Markdown body from the provided read-only buffer or fallback text.
    /// 根据提供的只读缓冲区或回退文本构建一块可复用的可选 Markdown 正文。
    pub(super) fn view_workbench_markdown_body<'a>(
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
    pub(super) fn view_workbench_raw_body<'a>(
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

    /// Builds one reusable code-preview body that matches the patch/scaffold preview surface while keeping transcript text selectable.
    /// 构建一块与补丁/骨架预览表面一致且仍保留可选能力的可复用代码预览正文。
    pub(super) fn view_workbench_code_preview_body<'a>(
        &'a self,
        buffer: Option<&'a text_editor::Content>,
        fallback: String,
        syntax_token: &'a str,
        on_action: impl Fn(text_editor::Action) -> Message + 'a,
    ) -> Element<'a, Message> {
        if let Some(buffer) = buffer {
            WheelPassthrough::new(
                text_editor(buffer)
                    .height(Shrink)
                    .padding(14)
                    .size(WORKBENCH_TRANSCRIPT_CODE_PREVIEW_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .wrapping(iced::advanced::text::Wrapping::None)
                    .font(theme::code_font())
                    .style(style::diff_code_editor_style)
                    .highlight_with::<iced::highlighter::Highlighter>(
                        iced::highlighter::Settings {
                            theme: theme::code_theme(&self.app_theme),
                            token: syntax_token.to_string(),
                        },
                        theme::code_highlight_format,
                    )
                    .on_action(on_action),
            )
            .into()
        } else {
            container(
                text(fallback)
                    .size(WORKBENCH_TRANSCRIPT_CODE_PREVIEW_TEXT_SIZE)
                    .line_height(Pixels(CODE_EDITOR_LINE_HEIGHT))
                    .font(theme::code_font())
                    .wrapping(iced::advanced::text::Wrapping::None),
            )
            .padding(14)
            .width(Fill)
            .into()
        }
    }

    /// Builds the selectable unified transcript panel for the selected session.
    /// 构建当前选中会话使用的可选统一转录面板。
    pub(super) fn view_selected_session_agent_document_surface(
        &self,
        session_id: SessionId,
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
            AgentTranscriptSurfaceMode::RawSource => (
                self.session_agent_view_raw_buffers.get(&session_id),
                Message::SelectedSessionDocumentRawSourceAction
                    as fn(text_editor::Action) -> Message,
                self.selected_session_agent_view_document()
                    .map(|document| agent_view_document_raw_source(&document))
                    .unwrap_or_default(),
            ),
        };

        let body = self.view_workbench_code_preview_body(
            buffer,
            fallback_text,
            "markdown",
            on_action,
        );
        let body_flow = column![
            body,
            space::vertical().height(self.transcript_bottom_spacer_height()),
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
        .into()
    }

    /// Builds the centered thinking modal shown when the user explicitly opens one inline thinking block.
    /// 构建当用户显式打开某个内联思考区块时显示的居中思考弹层。
    pub(super) fn view_selected_session_thinking_modal(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        let modal_state = self
            .active_thinking_modal
            .as_ref()
            .filter(|modal_state| modal_state.session_id == session_id)?;
        let progress = thinking_modal_progress(modal_state.started_at, modal_state.phase);
        Some(view_thinking_modal_surface(
            modal_state.payload.title.clone(),
            modal_state.payload.helper_text.clone(),
            &modal_state.payload.document,
            modal_state.payload.document_signature,
            &self.app_theme,
            progress,
            thinking_overlay_scrollable_id(session_id),
            move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            },
            |interaction| match interaction {
                SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                    Message::OpenSelectedSessionThinkingModal(block_id)
                }
                SelectableTranscriptInteraction::OpenToolModal(block_id) => {
                    Message::OpenSelectedSessionToolModal(block_id)
                }
                SelectableTranscriptInteraction::ToggleBlockInline(_) => Message::Noop,
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
            Message::CloseSelectedSessionThinkingModal,
        ))
    }

    /// Builds the centered tool modal shown when the user explicitly opens one inline tool block.
    /// 构建当用户显式打开某个内联工具区块时显示的居中工具弹层。
    pub(super) fn view_selected_session_tool_modal(
        &self,
        session_id: SessionId,
    ) -> Option<Element<'_, Message>> {
        let modal_state = self
            .active_tool_modal
            .as_ref()
            .filter(|modal_state| modal_state.session_id == session_id)?;
        let progress = thinking_modal_progress(modal_state.started_at, modal_state.phase);
        Some(view_thinking_modal_surface(
            modal_state.payload.title.clone(),
            modal_state.payload.helper_text.clone(),
            &modal_state.payload.document,
            modal_state.payload.document_signature,
            &self.app_theme,
            progress,
            thinking_overlay_scrollable_id(session_id),
            move |request| Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            },
            |interaction| match interaction {
                SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                    Message::OpenSelectedSessionThinkingModal(block_id)
                }
                SelectableTranscriptInteraction::OpenToolModal(block_id) => {
                    Message::OpenSelectedSessionToolModal(block_id)
                }
                SelectableTranscriptInteraction::ToggleBlockInline(_) => Message::Noop,
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
            Message::CloseSelectedSessionToolModal,
        ))
    }

    /// Builds the floating streaming-thinking overlay shown while one active thinking message is still in progress.
    /// 构建当某条活动思考消息仍在流式进行时显示的浮动思考面板。
    pub(super) fn view_selected_session_streaming_thinking_overlay(
        &self,
        session_id: SessionId,
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
                SelectableTranscriptInteraction::OpenThinkingModal(block_id) => {
                    Message::OpenSelectedSessionThinkingModal(block_id)
                }
                SelectableTranscriptInteraction::OpenToolModal(block_id) => {
                    Message::OpenSelectedSessionToolModal(block_id)
                }
                SelectableTranscriptInteraction::ToggleBlockInline(_) => Message::Noop,
            },
            move |viewport| Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            },
        ))
    }

    /// Builds the reusable workspace surface shown by the example panel.
    /// 构建由示例面板承载的可复用工作区主体。
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
    pub(super) fn view_luaskills_panel(&self) -> Element<'_, Message> {
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

    /// Builds the dedicated user-agent management panel backed by the dedicated user profile directory.
    /// 构建由独立用户 Agent 配置目录驱动的专用用户 Agent 管理面板。
    pub(super) fn view_agents_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::Agents.section_title()).size(22),
                text(MainDisplayPanel::Agents.description()).size(13),
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
    pub(super) fn view_luaskills_skill_card(
        &self,
        skill: &LuaSkillsCatalogSkill,
    ) -> Element<'_, Message> {
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
    pub(super) fn view_settings_panel(&self) -> Element<'_, Message> {
        let panel_header = container(
            column![
                text(MainDisplayPanel::Settings.section_title()).size(22),
                text(MainDisplayPanel::Settings.description()).size(13),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        column![
            panel_header,
            self.view_inspector(),
            self.view_archived_restore_card()
        ]
        .spacing(12)
        .height(Fill)
        .into()
    }

    /// Builds the dedicated archived-item restore card shown inside settings instead of the main session tree.
    /// 构建展示在设置页面中的独立归档恢复卡片，而不是继续显示在主会话树里。
    pub(super) fn view_archived_restore_card(&self) -> Element<'_, Message> {
        let archived_project_session_ids = self
            .archived_task_projects
            .iter()
            .flat_map(|project| project.session_ids.iter().copied())
            .collect::<HashSet<_>>();
        let standalone_archived_sessions = self
            .archived_sessions
            .iter()
            .filter(|session| !archived_project_session_ids.contains(&session.id))
            .collect::<Vec<_>>();

        let mut content = column![
            row![
                text("归档恢复").size(18),
                space::horizontal(),
                text("归档内容只在这里展示，主会话树不再显示。").size(12),
            ]
            .align_y(Center)
        ]
        .spacing(12);

        if self.archived_task_projects.is_empty() && standalone_archived_sessions.is_empty() {
            content = content.push(text("当前没有归档项目或归档会话。").size(13));
        } else {
            if !self.archived_task_projects.is_empty() {
                content = content.push(text("归档项目").size(14)).push(
                    self.archived_task_projects
                        .iter()
                        .fold(column![].spacing(1).width(Fill), |column, project| {
                            column.push(self.view_archived_task_project_row(project))
                        }),
                );
            }

            if !standalone_archived_sessions.is_empty() {
                content = content.push(text("独立归档会话").size(14)).push(
                    standalone_archived_sessions.into_iter().fold(
                        column![].spacing(1).width(Fill),
                        |column, session| {
                            column.push(self.view_archived_task_session_tree_row(session))
                        },
                    ),
                );
            }
        }

        if let Some(confirmation) = self.pending_archived_delete_confirmation.as_ref() {
            content = content
                .push(space::vertical().height(6))
                .push(self.view_archived_delete_confirmation_card(confirmation));
        }

        container(content)
            .padding(16)
            .style(container::bordered_box)
            .into()
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

    /// Builds the dedicated diff preview window so the panel can be iterated independently.
    /// 构建独立的 Diff 预览窗口，以便面板可以脱离主工作台单独迭代。
    pub(super) fn view_diff_window(&self, window_id: window::Id) -> Element<'_, Message> {
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

        Subscription::batch([
            window::close_events().map(Message::WindowClosed),
            window::resize_events()
                .map(|(window_id, size)| Message::WindowResized(window_id, size)),
            iced::event::listen_with(Self::map_main_panel_shortcut_event),
            iced::event::listen_with(Self::map_window_pointer_event),
            transcript_drag_auto_scroll_subscription,
            thinking_overlay_drag_auto_scroll_subscription,
            thinking_modal_animation_subscription,
        ])
    }

    /// Maps one raw runtime event into a top-level panel switch shortcut when the key combination matches.
    /// 当按键组合匹配时，将某个原始运行时事件映射为顶层面板切换快捷键消息。
    pub(super) fn map_main_panel_shortcut_event(
        event: iced::Event,
        _status: iced::event::Status,
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
            }) if !repeat => Some(Message::CloseSelectedSessionThinkingModal),
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
                && !modifiers.control()
                && !modifiers.shift()
                && !modifiers.alt()
                && !modifiers.logo() =>
            {
                Some(Message::ConfirmWorkbenchModelPickerSelection)
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
    pub(super) fn view_diff_panel(&self) -> Element<'_, Message> {
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
    pub(super) fn view_current_diff_file_bar(&self) -> Element<'_, Message> {
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
    pub(super) fn view_diff_file_card<'a>(
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
    pub(super) fn view_diff_file_header<'a>(
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
    pub(super) fn view_diff_file_header_content<'a>(
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
    pub(super) fn view_diff_file_header_metrics(
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
    pub(super) fn view_diff_file_code<'a>(
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
    pub(super) fn view_diff_gap_block(&self, omitted_lines: usize) -> Element<'_, Message> {
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
    pub(super) fn view_diff_hunk_block<'a>(
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
    pub(super) fn view_inspector(&self) -> Element<'_, Message> {
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

    /// Renders the MCP configuration card that exposes server summaries and one editable transport-aware server form.
    /// 渲染 MCP 配置卡片，以暴露服务器摘要与单个感知传输类型的可编辑服务器表单。
    pub(super) fn view_mcp_configuration_card(&self) -> Element<'_, Message> {
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

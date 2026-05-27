//! Composer, pending queue, and context-usage view composition for the main workbench.
//! 主工作台提示词输入区、待发送队列与上下文用量视图组合逻辑。

use super::*;

impl VulcanWorkbench {
    /// Returns whether the current composer buffer contains any submittable user instruction text.
    /// 返回当前提示词缓冲区是否包含任何可提交的用户指令文本。
    pub(in crate::app_shell) fn composer_has_submittable_content(&self) -> bool {
        composer_input_has_submittable_content(
            &self.composer_buffer.text(),
            &self.composer_attachments,
        )
    }

    /// Builds the compact send-or-stop button shown at the right side of prompt inputs.
    /// 构建显示在提示词输入框右侧的紧凑发送或停止按钮。
    pub(in crate::app_shell) fn view_run_trigger_button(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_stop_run_button(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_subagent_panel_return_button(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_subagent_panel_stop_button(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_subagent_panel_title_row(
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
    pub(in crate::app_shell) fn view_pending_prompt_queue_drop_indicator(
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
    pub(in crate::app_shell) fn view_pending_prompt_queue_row(
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
    pub(in crate::app_shell) fn view_pending_prompt_queue_tail_drop_zone(
        &self,
    ) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_pending_prompt_queue_strip(&self) -> Element<'_, Message> {
        let pending_entries = self.selected_session_pending_prompt_entries();
        if pending_entries.is_empty() {
            return space::vertical().height(0.0).into();
        }
        let pending_entry_count = pending_entries.len();
        let queue_body = pending_entries.into_iter().enumerate().fold(
            iced::widget::column![].spacing(0).width(Fill),
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
    pub(in crate::app_shell) fn view_workbench_model_selector(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_workbench_openrouter_endpoint_selector(
        &self,
    ) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_workbench_openrouter_endpoint_picker_modal(
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
                            iced::widget::column![
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
                iced::widget::column![
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
            iced::widget::column![
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
    pub(in crate::app_shell) fn view_workbench_reasoning_effort_selector(
        &self,
    ) -> Element<'_, Message> {
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
            iced::widget::column![
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
    pub(in crate::app_shell) fn view_composer_attachment_button(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_composer_attachment_strip(&self) -> Element<'_, Message> {
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

        container(
            iced::widget::column![icon, text(kind_label).size(10).color(icon_color)].spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(style::workbench_composer_attachment_icon_tile_style)
        .into()
    }

    /// Builds the lightweight drag-hover hint shown above the composer editor while files are hovering over the main window.
    /// 构建当文件正悬停在主窗口上方时显示在提示词编辑器上方的轻量拖拽提示。
    pub(in crate::app_shell) fn view_composer_drag_hover_hint(
        &self,
    ) -> Option<Element<'_, Message>> {
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
    pub(in crate::app_shell) fn view_composer_slash_command_panel(
        &self,
    ) -> Option<Element<'_, Message>> {
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
                            iced::widget::column![
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
                iced::widget::column![
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
    pub(in crate::app_shell) fn composer_line_count(&self) -> usize {
        composer_visible_line_count(&self.composer_buffer, self.composer_input_surface_width())
    }

    /// Returns the current dynamic height of the main prompt composer editor.
    /// 返回主提示词编辑器当前的动态高度。
    pub(in crate::app_shell) fn composer_editor_height(&self) -> f32 {
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
    pub(in crate::app_shell) fn composer_editor_scroll_metrics(&self) -> (f32, f32, f32) {
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
    pub(in crate::app_shell) fn composer_panel_width(&self) -> f32 {
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
    pub(in crate::app_shell) fn composer_panel_left(&self) -> f32 {
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
    pub(in crate::app_shell) fn floating_composer_footprint_height(&self) -> f32 {
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
    pub(in crate::app_shell) fn transcript_bottom_spacer_height(&self) -> f32 {
        TRANSCRIPT_BOTTOM_SPACER_HEIGHT + self.floating_composer_footprint_height()
    }

    /// Returns the actual inner input-surface width after subtracting the composer-panel horizontal padding.
    /// 返回扣除提示词面板水平内边距后的实际输入表面宽度。
    pub(in crate::app_shell) fn composer_input_surface_width(&self) -> f32 {
        self.composer_panel_width() - (COMPOSER_PANEL_EDGE_PADDING as f32 * 2.0)
    }

    /// Returns whether the main prompt composer should expand upward as a multi-line surface.
    /// 返回主提示词编辑器当前是否应作为向上展开的多行输入表面显示。
    pub(in crate::app_shell) fn composer_uses_expanded_surface(&self) -> bool {
        self.composer_line_count() > 1
    }

    /// Builds the shared main prompt composer editor surface.
    /// 构建主提示词编辑器共用的实际输入表面。
    pub(in crate::app_shell) fn view_composer_editor_surface(&self) -> Element<'_, Message> {
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
                        iced::widget::column![
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
    pub(in crate::app_shell) fn view_workbench_context_menu_overlay(
        &self,
    ) -> Option<Element<'_, Message>> {
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
    pub(in crate::app_shell) fn workbench_context_menu_position(
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
    pub(in crate::app_shell) fn view_workbench_context_menu(&self) -> Element<'_, Message> {
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
    pub(in crate::app_shell) fn view_workbench_context_menu_button(
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
    pub(in crate::app_shell) fn view_inline_composer_editor(&self) -> Element<'_, Message> {
        self.view_composer_editor_surface()
    }

    /// Builds the compact context-usage orb shown below the floating composer editor.
    /// 构建显示在悬浮输入编辑器下方的紧凑上下文占用圆盘。
    pub(in crate::app_shell) fn view_composer_context_usage_orb(&self) -> Element<'_, Message> {
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
            diameter: 34.0,
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

        if let Some(context_budget) = context_budget {
            let has_observed_usage = context_budget.observed_usage.is_some();
            let input_source_label = if has_observed_usage {
                "API实测"
            } else {
                "本地估算"
            };
            let status_label = match context_budget.severity {
                ContextBudgetSeverity::Healthy => "正常",
                ContextBudgetSeverity::Warning => "预警",
                ContextBudgetSeverity::CompactSoon => "建议压缩",
                ContextBudgetSeverity::Overflow => "预算溢出",
            };
            diagnostic_rows.push(format!(
                "输入占用：{:.0}% · {}",
                self.context_usage.clamp(0.0, 100.0),
                status_label
            ));
            diagnostic_rows.push(format!(
                "输入窗口（{}）：{}",
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
            if !context_budget.baseline_matches_active_runtime {
                diagnostic_rows.push(format!(
                    "恢复：基线失配（{}），已回退完整上下文。",
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
            let anchor_row = if let (Some(session_id), Some(context_start_message_id)) = (
                selected_session_id,
                runtime_state.context_start_message_id.as_deref(),
            ) {
                self.session_message_entry(session_id, context_start_message_id)
                    .map(|message| {
                        composer_context_anchor_reference_row(context_start_message_id, message)
                    })
                    .unwrap_or_else(|| {
                        format!(
                            "压缩锚点：已建立 · 关联消息 {}（已缺失）",
                            context_start_message_id
                        )
                    })
            } else if runtime_state.last_compaction_id.is_some() {
                String::from("压缩状态：已使用摘要前文")
            } else {
                String::from("压缩状态：尚未触发")
            };
            diagnostic_rows.push(anchor_row);
        }

        if let Some(preparation) = preparation {
            if let Some(context_baseline) = preparation.context_baseline.as_ref() {
                diagnostic_rows.push(format!("上下文层：{} 层", context_baseline.layer_count));
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
                    background: iced::color!(0x111827),
                    border: iced::color!(0xFDBA74),
                    track: iced::color!(0x334155),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                }
            }
            Some(context_budget) => match context_budget.severity {
                ContextBudgetSeverity::Healthy => ContextUsageOrbTone {
                    background: iced::color!(0x111827),
                    border: iced::color!(0x38BDF8),
                    track: iced::color!(0x334155),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::Warning => ContextUsageOrbTone {
                    background: iced::color!(0x111827),
                    border: iced::color!(0xFDE68A),
                    track: iced::color!(0x334155),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::CompactSoon => ContextUsageOrbTone {
                    background: iced::color!(0x111827),
                    border: iced::color!(0xFDBA74),
                    track: iced::color!(0x334155),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
                ContextBudgetSeverity::Overflow => ContextUsageOrbTone {
                    background: iced::color!(0x111827),
                    border: iced::color!(0xFDA29B),
                    track: iced::color!(0x334155),
                    progress: iced::color!(0x60A5FA),
                    text_primary: Color::WHITE,
                },
            },
            None => ContextUsageOrbTone {
                background: iced::color!(0x111827),
                border: iced::color!(0x64748B),
                track: iced::color!(0x334155),
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
}

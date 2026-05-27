//! Ask/question modal view composition for the main workbench.
//! 主工作台 ask/问题弹层的视图组合逻辑。

use super::*;

impl VulcanWorkbench {
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
                                    iced::widget::column![
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
                    iced::widget::column![
                        scrollable(
                            iced::widget::column(option_list.chain(std::iter::once(custom_row)))
                                .spacing(8)
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

                iced::widget::column![
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
                let summary_cards =
                    iced::widget::column(modal.request.questions.iter().enumerate().map(
                        |(question_index, question)| {
                            let draft = modal
                                .drafts
                                .get(question_index)
                                .cloned()
                                .unwrap_or_default();
                            let answers =
                                Self::collect_question_modal_prompt_answers(question, &draft)
                                    .unwrap_or_else(|_| vec![String::from("未填写")]);
                            container(
                                row![
                                    iced::widget::column![
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
                        iced::widget::column(
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
                                                iced::widget::column![
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

                iced::widget::column![
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
        let modal_body = iced::widget::column![
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
}

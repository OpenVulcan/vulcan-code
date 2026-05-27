//! Project settings and attachment cleanup view composition.
//! 项目设置与附件清理视图组合逻辑。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
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
                            let image_checkbox =
                                checkbox(self.project_attachment_cleanup_selection.image)
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
                            let audio_checkbox =
                                checkbox(self.project_attachment_cleanup_selection.audio)
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
                            let video_checkbox =
                                checkbox(self.project_attachment_cleanup_selection.video)
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
                            let pdf_checkbox =
                                checkbox(self.project_attachment_cleanup_selection.pdf)
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
}

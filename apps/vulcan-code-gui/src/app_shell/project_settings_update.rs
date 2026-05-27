//! Project settings and attachment cleanup message handling for the workbench update loop.
//! 工作台更新循环中的项目设置与附件清理消息处理。

use super::*;

impl VulcanWorkbench {
    /// Handles project settings and attachment cleanup messages outside the main update dispatcher.
    /// 在主更新分发器之外处理项目设置与附件清理消息。
    pub(super) fn update_project_settings_message(&mut self, message: Message) -> Task<Message> {
        match message {
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
            _ => Task::none(),
        }
    }
}

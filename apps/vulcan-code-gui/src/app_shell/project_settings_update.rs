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
                self.reload_project_vmm_settings_draft();
                Task::none()
            }
            Message::CloseTaskProjectSettings => {
                self.project_settings_project_id = None;
                self.project_attachment_cleanup_date_picker_open = false;
                Task::none()
            }
            Message::SetPromptSkillProjectState { name, enabled } => {
                match self.persist_prompt_skill_project_state(&name, enabled) {
                    Ok(true) => {
                        self.project_attachment_cleanup_notice =
                            format!("已更新项目级提示词技能 `{name}` 的启用状态。");
                    }
                    Ok(false) => {
                        self.project_attachment_cleanup_notice =
                            format!("项目级提示词技能 `{name}` 的启用状态未变化。");
                    }
                    Err(error) => {
                        self.project_attachment_cleanup_notice =
                            format!("保存项目级提示词技能 `{name}` 失败：{error}");
                    }
                }
                Task::none()
            }
            Message::ProjectVmmOverrideToggled(value) => {
                self.project_vmm_settings_draft.project_override_enabled = value;
                self.project_vmm_settings_draft.settings.notice = if value {
                    String::from("项目 VMM 覆盖已启用，点击保存后写入当前项目。")
                } else {
                    String::from("项目 VMM 覆盖已关闭，点击保存后继承全局配置。")
                };
                Task::none()
            }
            Message::ProjectVmmEnabledChanged(value) => {
                self.project_vmm_settings_draft.settings.enabled = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 启用状态已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmGrpcTargetChanged(value) => {
                let _discarded_project_transport_value = value;
                self.project_vmm_settings_draft.settings.notice = String::from(
                    "项目级 VMM 不再允许修改 gRPC 地址，请到全局 VMM 传输设置中修改。",
                );
                Task::none()
            }
            Message::ProjectVmmApiKeyChanged(value) => {
                let _discarded_project_transport_value = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目级 VMM 不再允许修改 API key，请到全局 VMM 传输设置中修改。");
                Task::none()
            }
            Message::ProjectVmmUserIdChanged(value) => {
                self.project_vmm_settings_draft.settings.user_id = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 用户 ID 已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmProjectIdChanged(value) => {
                self.project_vmm_settings_draft.settings.project_id = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 项目 ID 已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmReceiveTimeoutChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目级 VMM 不再允许修改请求超时，请到全局 VMM 传输设置中修改。");
                Task::none()
            }
            Message::ProjectVmmHandshakeTimeoutChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目级 VMM 不再允许修改握手超时，请到全局 VMM 传输设置中修改。");
                Task::none()
            }
            Message::ProjectVmmMaxReceiveMessageBytesChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice = String::from(
                    "项目级 VMM 不再允许修改最大接收字节数，请到全局 VMM 传输设置中修改。",
                );
                Task::none()
            }
            Message::ProjectVmmKeepaliveTimeChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice = String::from(
                    "项目级 VMM 不再允许修改 keepalive 间隔，请到全局 VMM 传输设置中修改。",
                );
                Task::none()
            }
            Message::ProjectVmmKeepaliveTimeoutChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice = String::from(
                    "项目级 VMM 不再允许修改 keepalive 超时，请到全局 VMM 传输设置中修改。",
                );
                Task::none()
            }
            Message::ProjectVmmKeepalivePermitWithoutCallsChanged(value) => {
                let _ = value;
                self.project_vmm_settings_draft.settings.notice = String::from(
                    "项目级 VMM 不再允许修改空闲 keepalive，请到全局 VMM 传输设置中修改。",
                );
                Task::none()
            }
            Message::ProjectVmmManagementUserRefChanged(value) => {
                self.project_vmm_settings_draft.settings.management_user_ref = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 管理用户引用已修改，可直接执行管理动作。");
                Task::none()
            }
            Message::ProjectVmmManagementProjectRefChanged(value) => {
                self.project_vmm_settings_draft
                    .settings
                    .management_project_ref = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 管理项目引用已修改，可直接执行管理动作。");
                Task::none()
            }
            Message::ProjectVmmImplicitTurnsChanged(value) => {
                self.project_vmm_settings_draft
                    .settings
                    .implicit_memory_turns = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 临时记忆 TTL 已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmProfileRefreshTurnsChanged(value) => {
                self.project_vmm_settings_draft
                    .settings
                    .profile_refresh_turns = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 画像刷新间隔已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmAiToolsChanged(value) => {
                self.project_vmm_settings_draft.settings.ai_tools_enabled = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM AI 专属工具开关已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmInjectionTagChanged(value) => {
                self.project_vmm_settings_draft.settings.injection_tag = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 注入标签已修改，点击保存后生效。");
                Task::none()
            }
            Message::ProjectVmmInjectionDescriptionChanged(value) => {
                self.project_vmm_settings_draft
                    .settings
                    .injection_description = value;
                self.project_vmm_settings_draft.settings.notice =
                    String::from("项目 VMM 注入说明已修改，点击保存后生效。");
                Task::none()
            }
            Message::ReloadProjectVmmConfiguration => {
                self.reload_project_vmm_settings_draft();
                self.project_attachment_cleanup_notice =
                    self.project_vmm_settings_draft.settings.notice.clone();
                Task::none()
            }
            Message::SaveProjectVmmConfiguration => {
                match self.save_project_vmm_settings_draft() {
                    Ok(()) => {
                        self.project_attachment_cleanup_notice =
                            self.project_vmm_settings_draft.settings.notice.clone();
                    }
                    Err(error) => {
                        self.project_vmm_settings_draft.settings.notice = error.clone();
                        self.project_attachment_cleanup_notice = error;
                    }
                }
                Task::none()
            }
            Message::RunProjectVmmManagement(action) => {
                let project_layer = match self.project_vmm_settings_draft.settings.to_layer() {
                    Ok(mut layer) => {
                        layer.grpc_target = None;
                        layer.grpc_api_key = None;
                        layer.handshake_timeout_ms = None;
                        layer.receive_timeout_ms = None;
                        layer.max_receive_message_bytes = None;
                        layer.grpc_keepalive_time_ms = None;
                        layer.grpc_keepalive_timeout_ms = None;
                        layer.grpc_keepalive_permit_without_calls = None;
                        layer
                    }
                    Err(error) => {
                        self.project_vmm_settings_draft.settings.notice = error.clone();
                        self.project_attachment_cleanup_notice = error;
                        return Task::none();
                    }
                };
                let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
                    &self.provider_configuration.vmm,
                    Some(&project_layer),
                    None,
                );
                let layer = vulcan_vmm::VmmConfigLayer {
                    enabled: Some(effective.enabled),
                    grpc_target: (!effective.grpc_target.trim().is_empty())
                        .then(|| effective.grpc_target.clone()),
                    grpc_api_key: effective.grpc_api_key.clone(),
                    handshake_timeout_ms: Some(effective.handshake_timeout_ms),
                    receive_timeout_ms: Some(effective.receive_timeout_ms),
                    max_receive_message_bytes: effective.max_receive_message_bytes,
                    grpc_keepalive_time_ms: Some(effective.grpc_keepalive_time_ms),
                    grpc_keepalive_timeout_ms: Some(effective.grpc_keepalive_timeout_ms),
                    grpc_keepalive_permit_without_calls: Some(
                        effective.grpc_keepalive_permit_without_calls,
                    ),
                    user_id: effective.user_id,
                    project_id: effective.project_id,
                    visible_memory_injection: Some(effective.visible_memory_injection),
                    implicit_memory_turns: Some(effective.implicit_memory_turns),
                    profile_refresh_turns: Some(effective.profile_refresh_turns),
                    session_compact_recall: Some(effective.session_compact_recall),
                    ai_tools_enabled: Some(effective.ai_tools_enabled),
                    injection_tag: Some(effective.injection_tag),
                    injection_description: Some(effective.injection_description),
                    install: Some(effective.install),
                };
                self.project_vmm_settings_draft.settings.notice =
                    format!("正在执行项目 VMM 管理动作：{}。", action.label());
                self.project_attachment_cleanup_notice =
                    self.project_vmm_settings_draft.settings.notice.clone();
                let management_user_ref = self
                    .project_vmm_settings_draft
                    .settings
                    .management_user_ref
                    .clone();
                let management_project_ref = self
                    .project_vmm_settings_draft
                    .settings
                    .management_project_ref
                    .clone();
                Task::perform(
                    vmm_management::run_vmm_management_action(
                        action,
                        layer,
                        management_user_ref,
                        management_project_ref,
                    ),
                    Message::ProjectVmmManagementFinished,
                )
            }
            Message::ProjectVmmManagementFinished(result) => {
                let notice = match result {
                    Ok(message) => message,
                    Err(error) => format!("项目 VMM 管理调用失败：{error}"),
                };
                self.project_vmm_settings_draft.settings.notice = notice.clone();
                self.project_attachment_cleanup_notice = notice;
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

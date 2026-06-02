//! Attachment cleanup and composer import helpers extracted from the main workbench module.
//! 从主工作台模块中抽取的附件清理与输入框导入辅助实现。

use super::*;

/// Stable project-scoped attachment-cleanup task context captured before a background preview or execution starts.
/// 在后台预览或执行开始前捕获的稳定项目级附件清理任务上下文。
#[derive(Debug, Clone)]
pub(super) struct ProjectAttachmentCleanupTaskContext {
    /// Stable project identifier that owns the cleanup task.
    /// 拥有该清理任务的稳定项目标识。
    pub(super) project_id: String,
    /// Human-readable project title used by notices and status rows.
    /// 供提示与状态栏复用的人类可读项目标题。
    pub(super) project_title: String,
    /// Absolute workspace path that owns the managed attachment store.
    /// 拥有托管附件存储的绝对工作区路径。
    pub(super) workspace_path: String,
    /// Canonical cutoff date string captured for the task.
    /// 为该任务捕获的规范截止日期字符串。
    pub(super) cutoff_date_text: String,
    /// Attachment-kind selection captured for the task.
    /// 为该任务捕获的附件类型选择状态。
    pub(super) selection: AttachmentCleanupSelection,
}

/// Validated cleanup execution request carried by the workbench confirmation modal.
/// 由工作台确认弹窗携带的已校验清理执行请求。
#[derive(Debug, Clone)]
pub(super) struct ProjectAttachmentCleanupExecutionRequest {
    /// Immutable task context used by the eventual background execution.
    /// 最终后台执行所使用的不可变任务上下文。
    pub(super) context: ProjectAttachmentCleanupTaskContext,
    /// Preview snapshot that the user explicitly confirmed before execution.
    /// 用户在执行前显式确认过的预览快照。
    pub(super) expected_preview: AttachmentCleanupPreview,
}

/// Completion outcome returned by one background project attachment cleanup execution.
/// 单次后台项目附件清理执行返回的完成结果。
#[derive(Debug, Clone)]
pub(super) enum ProjectAttachmentCleanupExecutionOutcome {
    /// Cleanup completed and optionally produced one follow-up preview built from the post-cleanup state.
    /// 清理已完成，并可选产出一份根据清理后状态构建的后续预览。
    Executed {
        /// Final cleanup execution report shown to the user.
        /// 展示给用户的最终清理执行报告。
        report: AttachmentCleanupReport,
        /// Best-effort refreshed preview built after execution completes.
        /// 执行完成后按最佳努力构建的刷新预览。
        refreshed_preview: Option<AttachmentCleanupPreview>,
    },
    /// Cleanup did not run because the revalidated preview changed before the destructive phase started.
    /// 由于破坏性阶段开始前重新校验得到的预览已变化，因此清理未真正执行。
    PreviewChanged {
        /// Fresh preview that superseded the previously confirmed snapshot.
        /// 取代旧确认快照的最新预览。
        preview: AttachmentCleanupPreview,
    },
}

impl VulcanWorkbench {
    /// Formats one raw attachment-cleanup byte count into a compact human-readable label for the settings card.
    /// 将原始附件清理字节数格式化为供设置卡片使用的紧凑可读标签。
    pub(super) fn format_attachment_cleanup_byte_len(byte_len: u64) -> String {
        const KIB: f64 = 1024.0;
        const MIB: f64 = KIB * 1024.0;
        const GIB: f64 = MIB * 1024.0;
        let byte_len_f64 = byte_len as f64;
        if byte_len_f64 >= GIB {
            format!("{:.2} GiB", byte_len_f64 / GIB)
        } else if byte_len_f64 >= MIB {
            format!("{:.2} MiB", byte_len_f64 / MIB)
        } else if byte_len_f64 >= KIB {
            format!("{:.2} KiB", byte_len_f64 / KIB)
        } else {
            format!("{byte_len} B")
        }
    }

    /// Formats one cleanup preview into a concise one-line summary used by both the notice bar and the settings detail card.
    /// 将清理预览格式化为一条紧凑单行摘要，供通知栏与设置详情卡片共同复用。
    pub(super) fn format_attachment_cleanup_preview_summary(
        &self,
        preview: &AttachmentCleanupPreview,
    ) -> String {
        let kind_summary = if preview.per_kind_counts.is_empty() {
            String::from("未匹配任何类型")
        } else {
            preview
                .per_kind_counts
                .iter()
                .map(|(kind, count)| format!("{kind}:{count}"))
                .collect::<Vec<_>>()
                .join(" / ")
        };
        format!(
            "截止 {cutoff} 将匹配 {count} 个附件、约 {byte_len}、影响 {sessions} 个会话（{kind_summary}）",
            cutoff = preview.cutoff_date_text,
            count = preview.matched_asset_count,
            byte_len = Self::format_attachment_cleanup_byte_len(preview.matched_byte_len),
            sessions = preview.impacted_session_count,
        )
    }

    /// Formats one cleanup execution report into a concise one-line summary used by the settings card and status bar.
    /// 将清理执行报告格式化为一条紧凑单行摘要，供设置卡片与状态栏共同复用。
    pub(super) fn format_attachment_cleanup_report_summary(
        &self,
        report: &AttachmentCleanupReport,
    ) -> String {
        // Extra summary suffix shown when replay has been revoked but the file could not be removed from disk.
        // 当回放已被撤销但文件无法从磁盘移除时展示的额外摘要后缀。
        let failed_delete_suffix = if report.failed_delete_count == 0 {
            String::new()
        } else {
            format!(
                "，{} 个文件删除失败但已撤销回放",
                report.failed_delete_count
            )
        };
        format!(
            "已按 {cutoff} 清理 {purged} 个附件，删除 {deleted} 个托管文件，{missing} 个文件已缺失，共释放约 {byte_len}，影响 {sessions} 个会话{failed_delete_suffix}",
            cutoff = report.cutoff_date_text,
            purged = report.purged_asset_count,
            deleted = report.deleted_file_count,
            missing = report.already_missing_file_count,
            byte_len = Self::format_attachment_cleanup_byte_len(report.purged_byte_len),
            sessions = report.impacted_session_count,
            failed_delete_suffix = failed_delete_suffix,
        )
    }

    /// Returns the resolved capability snapshot for the currently active workbench model when one is available.
    /// 返回当前激活工作台模型的已解析能力快照（如果存在）。
    pub(super) fn active_workbench_model_capabilities_snapshot(
        &self,
    ) -> Option<crate::provider_capabilities::ResolvedModelCapabilities> {
        let active_choice = self.active_workbench_model_choice()?;
        let definition = provider_config::supported_provider_by_id(&active_choice.provider_id)?;
        let model = self.resolve_workbench_model_entry(&active_choice);
        let runtime_options = self.current_pending_prompt_runtime_options();
        let effective_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(&runtime_options);
        let mut capabilities = resolve_effective_model_capabilities(
            definition,
            model.as_ref(),
            &effective_configuration,
        );
        self.apply_runtime_ai_capability_capability_overrides(&mut capabilities, &runtime_options);
        Some(capabilities)
    }

    /// Returns whether the currently active workbench model should accept one staged attachment kind according to declared input modalities.
    /// 返回当前激活工作台模型是否应根据声明输入模态接受某种已暂存附件类型。
    pub(super) fn active_workbench_supports_attachment_kind(
        &self,
        kind: ConversationAttachmentKind,
    ) -> bool {
        let Some(capabilities) = self.active_workbench_model_capabilities_snapshot() else {
            return false;
        };
        capabilities.allows_staged_attachment_kind(kind)
    }

    /// Returns whether the currently active workbench model should expose the generic attachment picker.
    /// 返回当前激活工作台模型是否应暴露通用附件选择器。
    pub(super) fn active_workbench_supports_any_attachments(&self) -> bool {
        [
            ConversationAttachmentKind::Audio,
            ConversationAttachmentKind::Image,
            ConversationAttachmentKind::Video,
            ConversationAttachmentKind::Pdf,
        ]
        .into_iter()
        .any(|kind| self.active_workbench_supports_attachment_kind(kind))
    }

    /// Returns the project currently bound to the project-settings modal.
    /// 返回当前绑定到项目设置弹窗的项目。
    pub(super) fn project_settings_task_project(&self) -> Option<&TaskProjectSummary> {
        let project_id = self.project_settings_project_id.as_deref()?;
        self.task_project_by_id(project_id)
    }

    /// Returns whether the project attachment cleanup card currently has a background preview or execution task in flight.
    /// 返回项目附件清理卡片当前是否存在后台预览或执行任务正在进行。
    pub(super) fn project_attachment_cleanup_busy(&self) -> bool {
        self.project_attachment_cleanup_preview_in_flight
            || self.project_attachment_cleanup_execution_in_flight
    }

    /// Builds the current project attachment cleanup task context from the visible project settings modal state.
    /// 根据当前可见的项目设置弹窗状态构建项目附件清理任务上下文。
    pub(super) fn active_project_attachment_cleanup_context(
        &self,
    ) -> Option<ProjectAttachmentCleanupTaskContext> {
        let project = self.project_settings_task_project()?;
        Some(ProjectAttachmentCleanupTaskContext {
            project_id: project.id.clone(),
            project_title: project.title.clone(),
            workspace_path: project.path.clone(),
            cutoff_date_text: self.project_attachment_cleanup_cutoff_date.to_string(),
            selection: self.project_attachment_cleanup_selection,
        })
    }

    /// Returns whether the provided cleanup context still matches the currently visible project and cleanup filters.
    /// 返回给定清理上下文是否仍与当前可见项目及清理筛选条件一致。
    pub(super) fn project_attachment_cleanup_context_matches_current(
        &self,
        context: &ProjectAttachmentCleanupTaskContext,
    ) -> bool {
        self.project_attachment_cleanup_project_matches_current(context)
            && self.project_attachment_cleanup_cutoff_date.to_string() == context.cutoff_date_text
            && self.project_attachment_cleanup_selection == context.selection
    }

    /// Returns whether the provided cleanup context still belongs to the project currently shown in the settings modal.
    /// 返回给定清理上下文是否仍属于当前设置弹窗展示的项目。
    pub(super) fn project_attachment_cleanup_project_matches_current(
        &self,
        context: &ProjectAttachmentCleanupTaskContext,
    ) -> bool {
        self.project_settings_project_id.as_deref() == Some(context.project_id.as_str())
    }

    /// Starts one background preview task for the current project attachment cleanup selection.
    /// 为当前项目附件清理选择启动一条后台预览任务。
    pub(super) fn start_project_attachment_cleanup_preview(
        &mut self,
        context: ProjectAttachmentCleanupTaskContext,
    ) -> Task<Message> {
        self.project_attachment_cleanup_preview_in_flight = true;
        self.project_attachment_cleanup_notice =
            format!("正在为项目 `{}` 生成附件清理预览。", context.project_title);
        self.status_message = self.project_attachment_cleanup_notice.clone();

        let task_context = context.clone();
        let message_context = context;
        Task::perform(
            async move {
                preview_attachment_cleanup(
                    &task_context.workspace_path,
                    &task_context.cutoff_date_text,
                    task_context.selection,
                )
            },
            move |result| {
                Message::ProjectAttachmentCleanupPreviewFinished(message_context.clone(), result)
            },
        )
    }

    /// Starts one background cleanup execution task after the user confirms the destructive action.
    /// 在用户确认破坏性操作后启动一条后台清理执行任务。
    pub(super) fn start_project_attachment_cleanup_execution(
        &mut self,
        request: ProjectAttachmentCleanupExecutionRequest,
    ) -> Task<Message> {
        self.project_attachment_cleanup_execution_in_flight = true;
        self.project_attachment_cleanup_notice = format!(
            "正在清理项目 `{}` 的历史附件缓存。",
            request.context.project_title
        );
        self.status_message = self.project_attachment_cleanup_notice.clone();

        let task_request = request.clone();
        let message_context = request.context;
        Task::perform(
            async move {
                let current_preview = preview_attachment_cleanup(
                    &task_request.context.workspace_path,
                    &task_request.context.cutoff_date_text,
                    task_request.context.selection,
                )?;
                if current_preview != task_request.expected_preview {
                    return Ok(ProjectAttachmentCleanupExecutionOutcome::PreviewChanged {
                        preview: current_preview,
                    });
                }

                let report = execute_attachment_cleanup(
                    &task_request.context.workspace_path,
                    &task_request.context.cutoff_date_text,
                    task_request.context.selection,
                )?;
                let refreshed_preview = preview_attachment_cleanup(
                    &task_request.context.workspace_path,
                    &task_request.context.cutoff_date_text,
                    task_request.context.selection,
                )
                .ok();
                Ok(ProjectAttachmentCleanupExecutionOutcome::Executed {
                    report,
                    refreshed_preview,
                })
            },
            move |result| {
                Message::ProjectAttachmentCleanupExecutionFinished(message_context.clone(), result)
            },
        )
    }

    /// Resolves the project-space root that should own managed attachment assets for one session.
    /// 解析单个会话的托管附件资产应归属的项目空间根目录。
    pub(super) fn attachment_workspace_path_for_session(&self, session_id: SessionId) -> String {
        self.session_by_id(session_id)
            .and_then(|session| session.project_id.as_deref())
            .and_then(|project_id| self.task_project_by_id(project_id))
            .map(|project| project.path.clone())
            .unwrap_or_else(|| self.workspace_path.trim().to_string())
    }

    /// Resolves the current composer attachment staging root, preferring the selected session project over the global workspace field.
    /// 解析当前提示词附件暂存根目录，优先使用选中会话所属项目而不是全局工作区输入框。
    pub(super) fn selected_attachment_workspace_path(&self) -> String {
        self.selected_session()
            .map(|session| session.id)
            .map(|session_id| self.attachment_workspace_path_for_session(session_id))
            .unwrap_or_else(|| self.workspace_path.trim().to_string())
    }

    /// Resolves a concise user-facing title for one attachment import error.
    /// 为单条附件导入错误解析面向用户的简洁标题。
    pub(super) fn composer_attachment_error_title(error: &str) -> &'static str {
        if error.contains("不是受支持的附件格式") {
            "不支持的附件格式"
        } else if error.contains("不是有效附件文件") {
            "附件文件无效"
        } else {
            "附件导入失败"
        }
    }

    /// Shows the standard attachment-limit notice for an import source that cannot accept more files.
    /// 为无法继续接收文件的导入来源显示标准附件数量上限提示。
    pub(super) fn show_composer_attachment_limit_notice(&mut self, source_label: &str) {
        let notice = format!(
            "当前已暂存 {} 个附件，已达到上限 {MAX_COMPOSER_ATTACHMENTS}；请先移除部分附件后再通过{source_label}继续添加。",
            self.composer_attachments.len()
        );
        self.show_workbench_notice_modal("附件数量已达上限", notice.clone());
        self.status_message = notice;
    }

    /// Validates whether every staged attachment still matches the active model modality declaration.
    /// 校验每条已暂存附件当前是否仍满足激活模型的模态声明。
    pub(super) fn validate_staged_attachments(
        &self,
        attachments: &[ConversationAttachment],
    ) -> Result<(), String> {
        if attachments.is_empty() {
            return Ok(());
        }
        if attachments.len() > MAX_COMPOSER_ATTACHMENTS {
            return Err(format!(
                "当前提示词最多支持 {MAX_COMPOSER_ATTACHMENTS} 个附件，请移除多余附件后再发送。"
            ));
        }
        for attachment in attachments {
            if !self.active_workbench_supports_attachment_kind(attachment.kind.clone()) {
                return Err(format!(
                    "当前模型未声明{}输入能力，无法发送附件 `{}`。",
                    attachment.kind.display_label(),
                    attachment.file_name
                ));
            }
        }
        Ok(())
    }

    /// Builds one best-effort stable deduplication key for one staged composer attachment so repeated imports of the same source file do not flood the attachment strip.
    /// 为单条已暂存提示词附件构建一条最佳努力的稳定去重键，避免同一源文件被重复导入后淹没附件条。
    pub(super) fn composer_attachment_identity_key(attachment: &ConversationAttachment) -> String {
        if let Some(source_path) = attachment
            .source_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return format!(
                "source|{}|{}|{}",
                attachment.kind.modality_name(),
                attachment.mime_type,
                source_path
            );
        }

        format!(
            "managed|{}|{}|{}|{:?}",
            attachment.kind.modality_name(),
            attachment.file_name,
            attachment.mime_type,
            attachment.byte_len
        )
    }

    /// Best-effort discards one newly staged attachment that was rejected before it entered the composer.
    /// 尽力丢弃一条在进入提示词输入框前被拒绝的新暂存附件。
    pub(super) fn discard_rejected_staged_composer_attachment(
        &mut self,
        attachment: &ConversationAttachment,
    ) {
        let attachment_workspace_path = self.selected_attachment_workspace_path();
        if let Err(error) = crate::attachment_asset_index::discard_unreferenced_staged_attachment(
            &attachment_workspace_path,
            attachment,
        ) {
            self.push_activity(
                ActivityLevel::Warning,
                "附件暂存清理失败",
                format!(
                    "附件 `{}` 未加入输入框，但清理其暂存文件失败：{error}",
                    attachment.file_name
                ),
            );
        }
    }

    /// Appends one imported attachment batch into the composer while enforcing model modality gates, deduplicating repeated file imports, and refreshing focus.
    /// 在强制模型模态门控、去重重复文件导入并刷新焦点的前提下，将一批导入附件追加到提示词输入框。
    pub(super) fn append_imported_composer_attachments(
        &mut self,
        attachments: Vec<ConversationAttachment>,
        source_label: &str,
    ) -> Task<Message> {
        if attachments.is_empty() {
            self.status_message = format!("{source_label}中没有可导入的附件。");
            return operation::focus(COMPOSER_EDITOR_ID);
        }

        let existing_keys = self
            .composer_attachments
            .iter()
            .map(Self::composer_attachment_identity_key)
            .collect::<BTreeSet<_>>();
        let mut next_keys = existing_keys;
        let mut additions = Vec::new();
        let mut duplicate_labels = Vec::new();
        let mut skipped_labels = Vec::new();
        let mut limit_labels = Vec::new();
        let remaining_slots =
            MAX_COMPOSER_ATTACHMENTS.saturating_sub(self.composer_attachments.len());

        for attachment in attachments {
            if !self.active_workbench_supports_attachment_kind(attachment.kind.clone()) {
                skipped_labels.push(format!(
                    "{} `{}`",
                    attachment.kind.display_label(),
                    attachment.file_name
                ));
                self.discard_rejected_staged_composer_attachment(&attachment);
                continue;
            }

            let identity_key = Self::composer_attachment_identity_key(&attachment);
            if !next_keys.insert(identity_key) {
                duplicate_labels.push(format!(
                    "{} `{}`",
                    attachment.kind.display_label(),
                    attachment.file_name
                ));
                self.discard_rejected_staged_composer_attachment(&attachment);
                continue;
            }

            if additions.len() >= remaining_slots {
                limit_labels.push(format!(
                    "{} `{}`",
                    attachment.kind.display_label(),
                    attachment.file_name
                ));
                self.discard_rejected_staged_composer_attachment(&attachment);
                continue;
            }

            additions.push(attachment);
        }

        if !additions.is_empty() {
            let workspace_path = self.selected_attachment_workspace_path();
            let composer_scope = self.composer_attachment_ref_scope();
            if let Err(error) = self.acquire_staged_attachment_refs(
                &workspace_path,
                &composer_scope,
                &additions,
                "输入框",
            ) {
                for attachment in &additions {
                    self.discard_rejected_staged_composer_attachment(attachment);
                }
                self.show_workbench_notice_modal("附件无法添加", error.clone());
                self.status_message = error;
                return operation::focus(COMPOSER_EDITOR_ID);
            }
            self.push_composer_undo_snapshot();
            self.composer_attachments.extend(additions.clone());
            if skipped_labels.is_empty() && limit_labels.is_empty() {
                self.workbench_notice_modal = None;
            }
        }

        self.dismiss_workbench_context_menu();
        self.composer_drag_hover_labels.clear();
        let mut status_parts = Vec::new();
        if additions.is_empty() {
            status_parts.push(format!("未添加任何{source_label}附件"));
        } else {
            status_parts.push(format!("已从{source_label}添加 {} 个附件", additions.len()));
        }
        if !duplicate_labels.is_empty() {
            status_parts.push(format!(
                "以下附件已存在并被跳过：{}",
                duplicate_labels.join("、")
            ));
        }
        if !skipped_labels.is_empty() {
            status_parts.push(format!(
                "以下附件因模型未声明对应模态而被跳过：{}",
                skipped_labels.join("、")
            ));
        }
        if !limit_labels.is_empty() {
            status_parts.push(format!(
                "附件数量上限为 {MAX_COMPOSER_ATTACHMENTS} 个，已跳过：{}",
                limit_labels.join("、")
            ));
        }
        self.status_message = format!("{}。", status_parts.join("；"));
        if !skipped_labels.is_empty() || !limit_labels.is_empty() {
            let mut notice_lines = Vec::new();
            if !limit_labels.is_empty() {
                notice_lines.push(format!(
                    "当前最多可保留 {MAX_COMPOSER_ATTACHMENTS} 个附件；已跳过：{}。",
                    limit_labels.join("、")
                ));
            }
            if !skipped_labels.is_empty() {
                notice_lines.push(format!(
                    "当前模型未声明对应输入模态；已跳过：{}。",
                    skipped_labels.join("、")
                ));
            }
            self.show_workbench_notice_modal("附件未全部添加", notice_lines.join("\n"));
        }

        let selected_session_id = self.selected_session().map(|session| session.id);
        self.save_selected_session_composer_draft();
        let persist_task = selected_session_id
            .map(|session_id| self.persist_single_session_runtime_state_task(session_id))
            .unwrap_or_else(Task::none);

        Task::batch([operation::focus(COMPOSER_EDITOR_ID), persist_task])
    }

    /// Stages one arbitrary RGBA image snapshot as a managed PNG attachment reusable by both iced clipboard images and native clipboard fallbacks.
    /// 将任意 RGBA 图片快照暂存为受管理的 PNG 附件，供 iced 剪贴板图片与原生剪贴板回退路径共同复用。
    pub(super) fn stage_composer_rgba_image_attachment(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<ConversationAttachment, String> {
        let descriptor =
            crate::attachment_assets::attachment_descriptor_from_mime_type("image/png")
                .ok_or_else(|| String::from("宿主暂不支持将剪贴板图片暂存为 PNG 附件。"))?;
        let png_bytes =
            crate::attachment_assets::encode_rgba_image_to_png_bytes(width, height, rgba)?;

        let attachment_workspace_path = self.selected_attachment_workspace_path();
        crate::attachment_assets::stage_attachment_bytes(
            &attachment_workspace_path,
            &descriptor,
            "clipboard-image.png",
            None,
            &png_bytes,
        )
    }

    /// Pastes the highest-priority clipboard payload into the composer, optionally restricting the operation to attachment-only imports.
    /// 将最高优先级的剪贴板载荷粘贴到提示词输入框，并可按需限制为仅导入附件。
    pub(super) fn paste_composer_clipboard(&mut self, attachments_only: bool) -> Task<Message> {
        self.dismiss_workbench_context_menu();

        match read_composer_clipboard_payload() {
            Ok(ComposerClipboardPayload::Files(files)) => {
                if self.composer_attachments.len() >= MAX_COMPOSER_ATTACHMENTS {
                    self.show_composer_attachment_limit_notice("剪贴板文件");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                let attachment_workspace_path = self.selected_attachment_workspace_path();
                match stage_composer_attachment_paths(&attachment_workspace_path, &files) {
                    Ok(attachments) => {
                        self.append_imported_composer_attachments(attachments, "剪贴板文件")
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal(
                            Self::composer_attachment_error_title(&error),
                            error.clone(),
                        );
                        self.status_message = format!("粘贴剪贴板文件失败：{error}");
                        operation::focus(COMPOSER_EDITOR_ID)
                    }
                }
            }
            Ok(ComposerClipboardPayload::Image {
                width,
                height,
                rgba,
            }) => {
                if self.composer_attachments.len() >= MAX_COMPOSER_ATTACHMENTS {
                    self.show_composer_attachment_limit_notice("剪贴板图片");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                match self.stage_composer_rgba_image_attachment(width, height, rgba.as_slice()) {
                    Ok(attachment) => {
                        self.append_imported_composer_attachments(vec![attachment], "剪贴板图片")
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal("剪贴板图片导入失败", error.clone());
                        self.status_message = format!("粘贴剪贴板图片失败：{error}");
                        operation::focus(COMPOSER_EDITOR_ID)
                    }
                }
            }
            Ok(ComposerClipboardPayload::Text(text)) => {
                if attachments_only {
                    self.status_message =
                        String::from("当前剪贴板中没有可导入的图片、音频、视频或 PDF 附件。");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }

                if !composer_clipboard_text_is_pastable(Some(&text)) {
                    self.status_message = String::from("当前剪贴板中的文本为空，未执行粘贴。");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }

                let action_task = self.apply_composer_editor_action(text_editor::Action::Edit(
                    text_editor::Edit::Paste(std::sync::Arc::new(text)),
                ));
                Task::batch([action_task, operation::focus(COMPOSER_EDITOR_ID)])
            }
            Ok(ComposerClipboardPayload::Empty) => {
                self.status_message = if attachments_only {
                    String::from("当前剪贴板中没有可导入的图片、音频、视频或 PDF 附件。")
                } else {
                    String::from(
                        "当前剪贴板中没有可粘贴的文本，也没有可导入的图片、音频、视频或 PDF 附件。",
                    )
                };
                operation::focus(COMPOSER_EDITOR_ID)
            }
            Err(error) => {
                self.status_message = if attachments_only {
                    format!("读取附件剪贴板失败：{error}")
                } else {
                    format!("读取剪贴板失败：{error}")
                };
                operation::focus(COMPOSER_EDITOR_ID)
            }
        }
    }

    /// Tracks one hovered file label so the floating composer panel can surface a drop hint before the user releases the pointer.
    /// 跟踪一条悬停文件标签，使浮动提示词面板能在用户释放指针前展示拖放提示。
    pub(super) fn push_composer_drag_hover_label(&mut self, path: &Path) {
        let label = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if self
            .composer_drag_hover_labels
            .iter()
            .any(|value| value == &label)
        {
            return;
        }
        self.composer_drag_hover_labels.push(label);
    }

    /// Opens the native attachment picker and appends unique supported attachments into the composer draft.
    /// 打开原生附件选择器，并向提示词草稿追加不重复且受支持的附件。
    pub(super) fn add_composer_attachments(&mut self) -> Task<Message> {
        if !self.active_workbench_supports_any_attachments() {
            let notice = String::from(
                "当前模型未声明可用附件输入模态，请切换到支持音频、图片、视频或 PDF 的模型后再试。",
            );
            self.show_workbench_notice_modal("当前模型不支持附件", notice.clone());
            self.status_message = notice;
            return Task::none();
        }
        if self.composer_attachments.len() >= MAX_COMPOSER_ATTACHMENTS {
            self.show_composer_attachment_limit_notice("文件选择");
            return Task::none();
        }

        let attachment_workspace_path = self.selected_attachment_workspace_path();
        match pick_composer_attachments(&attachment_workspace_path) {
            Ok(Some(attachments)) => {
                self.append_imported_composer_attachments(attachments, "文件选择")
            }
            Ok(None) => {
                self.status_message = String::from("已取消添加附件。");
                Task::none()
            }
            Err(error) => {
                self.show_workbench_notice_modal(
                    Self::composer_attachment_error_title(&error),
                    error.clone(),
                );
                self.status_message = format!("添加附件失败：{error}");
                Task::none()
            }
        }
    }

    /// Removes one staged composer attachment and keeps undo state available for recovery.
    /// 移除一条已暂存的提示词附件，并保留可恢复的撤销状态。
    pub(super) fn remove_composer_attachment(&mut self, local_path: &str) -> Task<Message> {
        let Some(index) = self
            .composer_attachments
            .iter()
            .position(|attachment| attachment.local_path == local_path)
        else {
            self.status_message = String::from("未找到要移除的附件。");
            return Task::none();
        };

        self.push_composer_undo_snapshot();
        let removed = self.composer_attachments.remove(index);
        let workspace_path = self.selected_attachment_workspace_path();
        let composer_scope = self.composer_attachment_ref_scope();
        self.release_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            std::slice::from_ref(&removed),
            "输入框",
        );
        if self
            .workbench_image_preview_modal
            .as_ref()
            .is_some_and(|modal| modal.local_path == removed.local_path)
        {
            self.workbench_image_preview_modal = None;
        }
        self.status_message = format!(
            "已移除{}附件 `{}`。",
            removed.kind.display_label(),
            removed.file_name
        );
        let selected_session_id = self.selected_session().map(|session| session.id);
        self.save_selected_session_composer_draft();
        selected_session_id
            .map(|session_id| self.persist_single_session_runtime_state_task(session_id))
            .unwrap_or_else(Task::none)
    }
}

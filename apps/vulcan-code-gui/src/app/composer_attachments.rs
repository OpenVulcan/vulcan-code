//! Composer attachment lifecycle helpers extracted from the main workbench module.
//! 从主工作台模块中抽取的输入框附件生命周期辅助实现。

use super::*;

impl VulcanWorkbench {
/// Builds the transient lease scope used by the currently selected composer attachment strip.
    /// 构建当前选中输入框附件条使用的瞬时租约作用域。
    pub(super) fn composer_attachment_ref_scope(&self) -> String {
        self.selected_session()
            .map(|session| format!("composer:{}", session.id))
            .unwrap_or_else(|| String::from("composer:workspace"))
    }

    /// Builds one unique transient lease scope for a composer undo snapshot.
    /// 为输入框撤销快照构建一个唯一瞬时租约作用域。
    pub(super) fn composer_undo_attachment_ref_scope() -> String {
        format!(
            "composer-undo:{}:{}",
            current_unix_millis_text(),
            Uuid::new_v4()
        )
    }

    /// Builds the transient lease scope for one visible pending-prompt panel entry.
    /// 构建一条可见待发送面板项的瞬时租约作用域。
    pub(super) fn pending_panel_attachment_ref_scope(session_id: SessionId, entry_id: &str) -> String {
        format!("pending-panel:{session_id}:{entry_id}")
    }

    /// Builds the transient lease scope for one internal pending-prompt queue entry.
    /// 构建一条内部待发送队列项的瞬时租约作用域。
    pub(super) fn pending_internal_attachment_ref_scope(session_id: SessionId, entry_id: &str) -> String {
        format!("pending-internal:{session_id}:{entry_id}")
    }

    /// Builds the transient lease scope for one prompt entry that is being dispatched immediately.
    /// 构建一条正在立即派发的提示词项的瞬时租约作用域。
    pub(super) fn prompt_dispatch_attachment_ref_scope(session_id: SessionId, entry_id: &str) -> String {
        format!("dispatch:{session_id}:{entry_id}")
    }

    /// Adds one staging lease scope for a batch of attachments before the caller mutates UI ownership.
    /// 在调用方改变界面所有权前，为一批附件增加一个暂存租约作用域。
    pub(super) fn acquire_staged_attachment_refs(
        &mut self,
        workspace_path: &str,
        scope: &str,
        attachments: &[ConversationAttachment],
        label: &str,
    ) -> Result<(), String> {
        if attachments.is_empty() {
            return Ok(());
        }
        crate::attachment_asset_index::acquire_staged_attachment_ref(
            workspace_path,
            scope,
            attachments,
        )
        .map(|_| ())
        .map_err(|error| {
            let message = format!("{label} 附件租约 `{scope}` 记录失败：{error}");
            self.push_activity(ActivityLevel::Warning, "附件租约记录失败", message.clone());
            message
        })
    }

    /// Releases one staging lease scope for a batch of attachments and reports best-effort failures.
    /// 为一批附件释放一个暂存租约作用域，并以尽力方式上报失败。
    pub(super) fn release_staged_attachment_refs(
        &mut self,
        workspace_path: &str,
        scope: &str,
        attachments: &[ConversationAttachment],
        label: &str,
    ) {
        if attachments.is_empty() {
            return;
        }
        if let Err(error) = crate::attachment_asset_index::release_staged_attachment_ref(
            workspace_path,
            scope,
            attachments,
        ) {
            self.push_activity(
                ActivityLevel::Warning,
                "附件租约释放失败",
                format!("{label} 附件租约 `{scope}` 释放失败：{error}"),
            );
        }
    }

    /// Builds a stable key used to compare staged attachment lease ownership across composer replacements.
    /// 构建用于在输入框替换期间比较已暂存附件租约归属的稳定键。
    pub(super) fn staged_attachment_lease_key(attachment: &ConversationAttachment) -> String {
        if let Some(asset_id) = attachment
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return format!("asset|{asset_id}");
        }
        format!(
            "identity|{}",
            Self::composer_attachment_identity_key(attachment)
        )
    }

    /// Returns only attachments whose current scope can be released because the replacement set no longer retains them.
    /// 仅返回可释放当前作用域的附件，因为替换后的集合已经不再持有它们。
    pub(super) fn staged_attachments_released_after_replacement(
        release_candidates: &[ConversationAttachment],
        retained_attachments: &[ConversationAttachment],
    ) -> Vec<ConversationAttachment> {
        let retained_keys = retained_attachments
            .iter()
            .map(Self::staged_attachment_lease_key)
            .collect::<BTreeSet<_>>();
        release_candidates
            .iter()
            .filter(|attachment| {
                !retained_keys.contains(&Self::staged_attachment_lease_key(attachment))
            })
            .cloned()
            .collect()
    }

    /// Releases all undo snapshot attachment leases retained by the composer history.
    /// 释放输入框撤销历史持有的全部附件租约。
    pub(super) fn release_all_composer_undo_attachment_refs(&mut self) {
        let snapshots = self.composer_undo_history.drain(..).collect::<Vec<_>>();
        for snapshot in snapshots {
            if let (Some(scope), Some(workspace_path)) = (
                snapshot.attachment_ref_scope.as_deref(),
                snapshot.attachment_workspace_path.as_deref(),
            ) {
                self.release_staged_attachment_refs(
                    workspace_path,
                    scope,
                    &snapshot.attachments,
                    "输入框撤销",
                );
            }
        }
    }

    /// Releases transient dispatch/internal queue leases for one prompt entry.
    /// 释放一条提示词项的瞬时派发与内部队列租约。
    pub(super) fn release_prompt_entry_transient_attachment_refs(
        &mut self,
        session_id: SessionId,
        prompt_entry: &PendingSessionPromptEntry,
    ) {
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let internal_scope =
            Self::pending_internal_attachment_ref_scope(session_id, &prompt_entry.id);
        let dispatch_scope =
            Self::prompt_dispatch_attachment_ref_scope(session_id, &prompt_entry.id);
        self.release_staged_attachment_refs(
            &workspace_path,
            &internal_scope,
            &prompt_entry.attachments,
            "内部待发送队列",
        );
        self.release_staged_attachment_refs(
            &workspace_path,
            &dispatch_scope,
            &prompt_entry.attachments,
            "提示词派发",
        );
    }

    /// Captures the current composer contents into one undo snapshot before a mutating action.
    /// 在某个会修改提示词内容的动作之前，将当前提示词内容捕获为一条撤销快照。
    pub(super) fn push_composer_undo_snapshot(&mut self) {
        let mut snapshot = ComposerUndoSnapshot {
            text: self.composer_buffer.text(),
            attachments: self.composer_attachments.clone(),
            cursor: self.composer_buffer.cursor(),
            scroll_offset: self.composer_scroll_offset,
            attachment_ref_scope: None,
            attachment_workspace_path: None,
        };

        if self.composer_undo_history.last().is_some_and(|last| {
            last.text == snapshot.text
                && last.attachments == snapshot.attachments
                && last.cursor == snapshot.cursor
        }) {
            return;
        }

        if !snapshot.attachments.is_empty() {
            let workspace_path = self.selected_attachment_workspace_path();
            let scope = Self::composer_undo_attachment_ref_scope();
            if self
                .acquire_staged_attachment_refs(
                    &workspace_path,
                    &scope,
                    &snapshot.attachments,
                    "输入框撤销",
                )
                .is_ok()
            {
                snapshot.attachment_ref_scope = Some(scope);
                snapshot.attachment_workspace_path = Some(workspace_path);
            } else {
                snapshot.attachments.clear();
            }
        }

        self.composer_undo_history.push(snapshot);

        if self.composer_undo_history.len() > COMPOSER_UNDO_HISTORY_LIMIT {
            let overflow = self.composer_undo_history.len() - COMPOSER_UNDO_HISTORY_LIMIT;
            let removed_snapshots = self
                .composer_undo_history
                .drain(0..overflow)
                .collect::<Vec<_>>();
            for removed_snapshot in removed_snapshots {
                if let (Some(scope), Some(workspace_path)) = (
                    removed_snapshot.attachment_ref_scope.as_deref(),
                    removed_snapshot.attachment_workspace_path.as_deref(),
                ) {
                    self.release_staged_attachment_refs(
                        workspace_path,
                        scope,
                        &removed_snapshot.attachments,
                        "输入框撤销",
                    );
                }
            }
        }
    }

    /// Restores the latest retained composer undo snapshot when one exists.
    /// 在存在撤销快照时恢复最近保留的一条提示词撤销快照。
    pub(super) fn restore_latest_composer_undo_snapshot(&mut self) -> Task<Message> {
        let Some(snapshot) = self.composer_undo_history.pop() else {
            return Task::none();
        };

        let workspace_path = self.selected_attachment_workspace_path();
        let composer_scope = self.composer_attachment_ref_scope();
        let previous_attachments = self.composer_attachments.clone();
        let snapshot_workspace_path = snapshot
            .attachment_workspace_path
            .clone()
            .unwrap_or_else(|| workspace_path.clone());
        if !snapshot.attachments.is_empty() && snapshot_workspace_path != workspace_path {
            if let Some(scope) = snapshot.attachment_ref_scope.as_deref() {
                self.release_staged_attachment_refs(
                    &snapshot_workspace_path,
                    scope,
                    &snapshot.attachments,
                    "输入框撤销",
                );
            }
            let message =
                String::from("撤销快照属于另一个项目，已丢弃该附件撤销记录以避免跨项目引用错误。");
            self.show_workbench_notice_modal("无法恢复跨项目附件", message.clone());
            self.status_message = message;
            return Task::none();
        }
        if let Err(error) = self.validate_staged_attachments(&snapshot.attachments) {
            if let Some(scope) = snapshot.attachment_ref_scope.as_deref() {
                self.release_staged_attachment_refs(
                    &snapshot_workspace_path,
                    scope,
                    &snapshot.attachments,
                    "输入框撤销",
                );
            }
            let message = format!("撤销快照中的附件已不符合当前模型能力：{error}");
            self.show_workbench_notice_modal("附件无法恢复", message.clone());
            self.status_message = message;
            return Task::none();
        }
        if let Err(error) = self.acquire_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            &snapshot.attachments,
            "输入框",
        ) {
            if let Some(scope) = snapshot.attachment_ref_scope.as_deref() {
                self.release_staged_attachment_refs(
                    &snapshot_workspace_path,
                    scope,
                    &snapshot.attachments,
                    "输入框撤销",
                );
            }
            self.show_workbench_notice_modal("附件无法恢复", error.clone());
            self.status_message = error;
            return Task::none();
        }
        let released_previous_attachments = Self::staged_attachments_released_after_replacement(
            &previous_attachments,
            &snapshot.attachments,
        );
        self.release_staged_attachment_refs(
            &workspace_path,
            &composer_scope,
            &released_previous_attachments,
            "输入框",
        );
        if let Some(scope) = snapshot.attachment_ref_scope.as_deref() {
            self.release_staged_attachment_refs(
                &snapshot_workspace_path,
                scope,
                &snapshot.attachments,
                "输入框撤销",
            );
        }

        let mut restored_buffer = text_editor::Content::with_text(&snapshot.text);
        restored_buffer.move_to(snapshot.cursor);
        self.composer_buffer = restored_buffer;
        self.composer = snapshot.text;
        self.composer_attachments = snapshot.attachments;
        self.composer_scroll_offset = snapshot.scroll_offset.max(0.0);
        self.dismiss_workbench_context_menu();

        operation::focus(COMPOSER_EDITOR_ID)
    }

    /// Applies one composer editor action while keeping undo history, focus, and mirrored scroll state in sync.
    /// 应用一条提示词编辑器动作，并同步维护撤销历史、焦点以及镜像滚动状态。
    pub(super) fn apply_composer_editor_action(&mut self, action: text_editor::Action) -> Task<Message> {
        let composer_surface_width = self.composer_input_surface_width();
        let action_is_scroll = matches!(action, text_editor::Action::Scroll { .. });
        let action_is_mutating = action.is_edit();
        let visible_line_count_before =
            composer_visible_line_count(&self.composer_buffer, composer_surface_width);
        let used_expanded_surface_before = visible_line_count_before > 1;
        let used_overflow_scroll_before = visible_line_count_before > 10;

        if action_is_mutating {
            self.push_composer_undo_snapshot();
        }

        self.composer_buffer.perform(action);
        self.composer = self.composer_buffer.text();

        let visible_line_count_after =
            composer_visible_line_count(&self.composer_buffer, composer_surface_width);
        let used_expanded_surface_after = visible_line_count_after > 1;
        let used_overflow_scroll_after = visible_line_count_after > 10;
        let mut tasks = Vec::new();

        if used_overflow_scroll_after {
            let (content_bounds, caret_position) =
                measure_composer_layout(&self.composer_buffer, composer_surface_width);
            let content_height = COMPOSER_EDITOR_VERTICAL_INSET
                + content_bounds.height.max(COMPOSER_EDITOR_LINE_HEIGHT);
            let viewport_height = self.composer_editor_height();
            let viewport_text_height =
                (viewport_height - COMPOSER_EDITOR_VERTICAL_INSET).max(COMPOSER_EDITOR_LINE_HEIGHT);
            let max_scroll_offset = (content_height - viewport_height).max(0.0);

            if action_is_scroll {
                let (_, actual_scroll_offset) = self.composer_buffer.plain_text_scroll_metrics();
                self.composer_scroll_offset = actual_scroll_offset.clamp(0.0, max_scroll_offset);
            } else {
                self.composer_scroll_offset = composer_caret_scroll_target(
                    caret_position.y.max(0.0),
                    self.composer_scroll_offset,
                    viewport_text_height,
                    max_scroll_offset,
                );
            }
        } else {
            self.composer_scroll_offset = 0.0;
            self.composer_scrollbar_drag_grab_offset = None;
        }

        if used_expanded_surface_before != used_expanded_surface_after
            || used_overflow_scroll_before != used_overflow_scroll_after
        {
            tasks.push(operation::focus(COMPOSER_EDITOR_ID));
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
}

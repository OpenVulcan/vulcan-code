//! Last user-message edit and rerun helpers for the workbench shell.
//! 工作台外壳中用于编辑并重跑最后用户消息的辅助逻辑。

use super::*;

/// Restorable draft captured from the last canonical user turn.
/// 从最后一轮规范用户输入中捕获的可恢复草稿。
#[derive(Debug, Clone)]
pub(super) struct LastUserMessageEditDraft {
    /// User-authored prompt text that should be restored into the composer.
    /// 需要恢复到输入框中的用户提示词文本。
    pub(super) prompt: String,
    /// Structured attachments that should stay bound to the restored prompt.
    /// 需要继续绑定到恢复提示词上的结构化附件集合。
    pub(super) attachments: Vec<ConversationAttachment>,
}

/// Runtime-only modal state used to edit the final user prompt without mutating history first.
/// 运行时专用弹窗状态，用于在不提前修改历史的前提下编辑最后用户提示词。
pub(super) struct LastUserMessageEditModalState {
    /// Session that owns the editable final user prompt.
    /// 拥有可编辑最后用户提示词的会话。
    pub(super) session_id: SessionId,
    /// Visible transcript message id that must still be the final user prompt on submit.
    /// 提交时仍必须保持为最后用户提示词的可见转录消息标识。
    pub(super) message_id: String,
    /// Original prompt text captured when the modal opened.
    /// 弹窗打开时捕获的原始提示词文本。
    pub(super) original_prompt: String,
    /// Editable prompt buffer owned by the modal instead of the main composer.
    /// 由弹窗持有而非主输入框持有的可编辑提示词缓冲区。
    pub(super) draft_buffer: text_editor::Content,
    /// Attachments retained from the original user prompt and sent with the edited prompt.
    /// 从原始用户提示词保留并随编辑后提示词发送的附件。
    pub(super) attachments: Vec<ConversationAttachment>,
    /// Current mirrored vertical scroll offset for the modal editor.
    /// 弹窗编辑器当前镜像纵向滚动偏移量。
    pub(super) scroll_offset: f32,
    /// Latest local pointer y position tracked over the modal scrollbar.
    /// 在弹窗滚动条上追踪到的最近一次局部指针纵向位置。
    pub(super) scrollbar_pointer_y: Option<f32>,
    /// Current grabbed offset inside the modal scrollbar thumb.
    /// 当前弹窗滚动条滑块内部被抓取的偏移量。
    pub(super) scrollbar_drag_grab_offset: Option<f32>,
}

/// Result produced after trimming the last user turn from the canonical state.
/// 从规范状态中裁剪最后一轮用户输入后产生的结果。
#[derive(Debug, Clone)]
struct LastUserTurnRollback {
    /// Draft that should be restored before the next rerun is submitted.
    /// 下一次重跑提交前应恢复的草稿。
    draft: LastUserMessageEditDraft,
    /// Record identifiers removed from transcript-side caches.
    /// 已从转录侧缓存中移除的记录标识集合。
    removed_record_ids: Vec<String>,
}

impl VulcanWorkbench {
    /// Opens the last-user edit modal without rolling back conversation history.
    /// 打开最后用户消息编辑弹窗，但不回滚会话历史。
    pub(super) fn open_last_user_message_edit_modal(
        &mut self,
        message_id: String,
    ) -> Task<Message> {
        let Some((session_id, session_title)) = self
            .selected_session()
            .map(|session| (session.id, session.title.clone()))
        else {
            self.status_message = String::from("当前没有可编辑的会话。");
            return Task::none();
        };

        let draft = match self.last_user_message_edit_draft_for_message(session_id, &message_id) {
            Ok(draft) => draft,
            Err(error) => {
                self.status_message = error.clone();
                self.show_workbench_notice_modal("无法编辑最后用户消息", error);
                return Task::none();
            }
        };
        self.last_user_message_edit_modal = Some(LastUserMessageEditModalState {
            session_id,
            message_id,
            original_prompt: draft.prompt.clone(),
            draft_buffer: text_editor::Content::with_text(&draft.prompt),
            attachments: draft.attachments,
            scroll_offset: 0.0,
            scrollbar_pointer_y: None,
            scrollbar_drag_grab_offset: None,
        });
        self.status_message = format!("正在编辑 `{session_title}` 的最后一条用户消息。");
        Task::none()
    }

    /// Applies one text-editor action to the last-user edit modal buffer.
    /// 将一次文本编辑器动作应用到最后用户消息编辑弹窗缓冲区。
    pub(super) fn apply_last_user_message_edit_action(
        &mut self,
        action: text_editor::Action,
    ) -> Task<Message> {
        if let Some(modal) = self.last_user_message_edit_modal.as_mut() {
            let action_is_scroll = matches!(action, text_editor::Action::Scroll { .. });
            let action_is_mutating = action.is_edit();
            modal.draft_buffer.perform(action);
            let (content_bounds, caret_position) = measure_composer_layout(
                &modal.draft_buffer,
                LAST_USER_MESSAGE_EDIT_EDITOR_SURFACE_WIDTH,
            );
            let content_height = COMPOSER_EDITOR_VERTICAL_INSET
                + content_bounds.height.max(COMPOSER_EDITOR_LINE_HEIGHT);
            let viewport_height = LAST_USER_MESSAGE_EDIT_EDITOR_HEIGHT;
            let viewport_text_height =
                (viewport_height - COMPOSER_EDITOR_VERTICAL_INSET).max(COMPOSER_EDITOR_LINE_HEIGHT);
            let max_scroll_offset = (content_height - viewport_height).max(0.0);
            if max_scroll_offset > 0.5 {
                if action_is_scroll {
                    let (_, actual_scroll_offset) = modal.draft_buffer.plain_text_scroll_metrics();
                    modal.scroll_offset = actual_scroll_offset.clamp(0.0, max_scroll_offset);
                } else if action_is_mutating {
                    modal.scroll_offset = composer_caret_scroll_target(
                        caret_position.y.max(0.0),
                        modal.scroll_offset,
                        viewport_text_height,
                        max_scroll_offset,
                    );
                    modal
                        .draft_buffer
                        .scroll_to_plain_text_offset(modal.scroll_offset);
                }
            } else {
                modal.scroll_offset = 0.0;
                modal.scrollbar_drag_grab_offset = None;
            }
        }
        Task::none()
    }

    /// Updates the edit-modal scrollbar hover or drag state from a local pointer y position.
    /// 根据局部指针纵向位置更新编辑弹窗滚动条悬停或拖拽状态。
    pub(super) fn update_last_user_message_edit_scrollbar_pointer(
        &mut self,
        pointer_y: f32,
    ) -> Task<Message> {
        if let Some(modal) = self.last_user_message_edit_modal.as_mut() {
            modal.scrollbar_pointer_y = Some(pointer_y);
        }
        self.apply_last_user_message_edit_scrollbar_pointer_position(pointer_y);
        Task::none()
    }

    /// Starts dragging the edit-modal scrollbar thumb.
    /// 开始拖拽编辑弹窗滚动条滑块。
    pub(super) fn start_last_user_message_edit_scrollbar_drag(&mut self) -> Task<Message> {
        let Some(layout) = self.last_user_message_edit_scrollbar_layout() else {
            return Task::none();
        };
        let Some(modal) = self.last_user_message_edit_modal.as_mut() else {
            return Task::none();
        };
        let pointer_y = modal.scrollbar_pointer_y.unwrap_or(8.0);
        let local_track_y = (pointer_y - 8.0).clamp(0.0, layout.track_height);
        let thumb_bottom = layout.thumb_top + layout.thumb_height;
        let grab_offset = if local_track_y >= layout.thumb_top && local_track_y <= thumb_bottom {
            local_track_y - layout.thumb_top
        } else {
            (layout.thumb_height / 2.0).clamp(0.0, layout.thumb_height)
        };

        modal.scrollbar_drag_grab_offset = Some(grab_offset);
        self.apply_last_user_message_edit_scrollbar_pointer_position(pointer_y);
        Task::none()
    }

    /// Returns the current scrollbar layout for the last-user-message edit modal editor.
    /// 返回最后用户消息编辑弹窗输入区当前的滚动条布局。
    pub(super) fn last_user_message_edit_scrollbar_layout(
        &self,
    ) -> Option<ComposerScrollbarLayout> {
        let modal = self.last_user_message_edit_modal.as_ref()?;
        let (content_bounds, _) = measure_composer_layout(
            &modal.draft_buffer,
            LAST_USER_MESSAGE_EDIT_EDITOR_SURFACE_WIDTH,
        );
        let content_height =
            COMPOSER_EDITOR_VERTICAL_INSET + content_bounds.height.max(COMPOSER_EDITOR_LINE_HEIGHT);
        let viewport_height = LAST_USER_MESSAGE_EDIT_EDITOR_HEIGHT;
        let (_, scroll_offset) = modal.draft_buffer.plain_text_scroll_metrics();
        let max_scroll_offset = (content_height - viewport_height).max(0.0);
        if max_scroll_offset <= 0.5 {
            return None;
        }

        let track_height = (viewport_height - 16.0).max(0.0);
        let thumb_height =
            ((viewport_height / content_height) * track_height).clamp(24.0, track_height);
        let thumb_travel = (track_height - thumb_height).max(0.0);
        let thumb_top = if thumb_travel > 0.0 {
            (scroll_offset / max_scroll_offset).clamp(0.0, 1.0) * thumb_travel
        } else {
            0.0
        };

        Some(ComposerScrollbarLayout {
            track_height,
            thumb_height,
            thumb_travel,
            thumb_top,
            max_scroll_offset,
        })
    }

    /// Applies one local scrollbar pointer position to the modal editor scroll offset.
    /// 将一次局部滚动条指针位置应用为弹窗编辑器滚动偏移量。
    fn apply_last_user_message_edit_scrollbar_pointer_position(&mut self, pointer_y: f32) {
        let Some(layout) = self.last_user_message_edit_scrollbar_layout() else {
            if let Some(modal) = self.last_user_message_edit_modal.as_mut() {
                modal.scrollbar_drag_grab_offset = None;
            }
            return;
        };
        let Some(modal) = self.last_user_message_edit_modal.as_mut() else {
            return;
        };
        let Some(grab_offset) = modal.scrollbar_drag_grab_offset else {
            return;
        };

        let local_track_y = (pointer_y - 8.0).clamp(0.0, layout.track_height);
        let thumb_top = (local_track_y - grab_offset).clamp(0.0, layout.thumb_travel);
        let target_scroll_offset = if layout.thumb_travel > 0.0 {
            (thumb_top / layout.thumb_travel) * layout.max_scroll_offset
        } else {
            0.0
        };

        modal
            .draft_buffer
            .scroll_to_plain_text_offset(target_scroll_offset);
        let (_, actual_scroll_offset) = modal.draft_buffer.plain_text_scroll_metrics();
        modal.scroll_offset = actual_scroll_offset.clamp(0.0, layout.max_scroll_offset);
    }

    /// Closes the last-user edit modal without mutating conversation state.
    /// 关闭最后用户消息编辑弹窗且不修改会话状态。
    pub(super) fn cancel_last_user_message_edit(&mut self) -> Task<Message> {
        self.last_user_message_edit_modal = None;
        self.status_message = String::from("已取消编辑最后用户消息，历史未发生变化。");
        Task::none()
    }

    /// Confirms the modal draft, then rolls back and reruns the final user turn atomically from the user's perspective.
    /// 确认弹窗草稿，然后从用户视角原子地回滚并重跑最后用户轮次。
    pub(super) fn submit_last_user_message_edit(&mut self) -> Task<Message> {
        let Some(modal) = self.last_user_message_edit_modal.as_ref() else {
            return Task::none();
        };
        let session_id = modal.session_id;
        let message_id = modal.message_id.clone();
        let prompt = modal.draft_buffer.text();
        let attachments = modal.attachments.clone();
        let original_prompt = modal.original_prompt.clone();
        if !composer_input_has_submittable_content(&prompt, &attachments) {
            self.status_message = String::from("编辑后的用户消息为空，无法重新发送。");
            self.show_workbench_notice_modal(
                "无法发送编辑版用户消息",
                "编辑后的用户消息没有可发送的文本或附件，请补充内容后再发送。",
            );
            return Task::none();
        }
        if let Err(error) = self.last_user_message_edit_draft_for_message(session_id, &message_id) {
            self.status_message = error.clone();
            self.show_workbench_notice_modal("无法发送编辑版用户消息", error);
            return Task::none();
        }
        let session_title = self
            .session_by_id(session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| String::from("未命名会话"));
        let prompt_entry = self.build_pending_prompt_entry(
            prompt,
            attachments,
            PendingSessionPromptSource::UserFollowup,
            None,
            None,
            None,
        );
        if let Err(error) = self.validate_prompt_entry_attachments(&prompt_entry) {
            self.status_message = error.clone();
            self.show_workbench_notice_modal("附件无法发送", error);
            return Task::none();
        }
        let rollback = match self.rollback_last_user_turn_for_edit(session_id) {
            Ok(rollback) => rollback,
            Err(error) => {
                self.status_message = error.clone();
                self.show_workbench_notice_modal("无法回滚最后用户消息", error);
                return Task::none();
            }
        };
        debug_assert_eq!(rollback.draft.prompt, original_prompt);
        self.last_user_message_edit_modal = None;
        self.push_activity(
            ActivityLevel::Info,
            "最后用户消息已编辑重发",
            "已在确认发送后回滚上一轮用户输入之后的输出、工具结果、统计与 continuation，并立即发送编辑版。",
        );
        self.status_message = String::from("已发送编辑版用户消息，正在重新执行。");
        self.begin_session_run_from_prompt_entry(session_id, session_title, prompt_entry)
    }

    /// Resolves the editable last-user draft and validates all rollback safety gates.
    /// 解析可编辑的最后用户草稿，并校验全部回滚安全门禁。
    fn last_user_message_edit_draft_for_message(
        &mut self,
        session_id: SessionId,
        message_id: &str,
    ) -> Result<LastUserMessageEditDraft, String> {
        self.validate_last_user_message_edit_gate(session_id, false)?;
        let runtime_state = self
            .session_runtime_state(session_id)
            .ok_or_else(|| String::from("当前会话尚未载入运行时上下文，无法安全回滚。"))?;
        let last_user_record_index =
            last_visible_user_record_index(&runtime_state.conversation_state)
                .ok_or_else(|| String::from("当前会话还没有可编辑的用户消息。"))?;
        let last_user_record = runtime_state
            .conversation_state
            .records
            .get(last_user_record_index)
            .ok_or_else(|| String::from("当前会话还没有可编辑的用户消息。"))?;
        if last_user_record.id != message_id {
            return Err(String::from(
                "只能编辑最后一条用户消息。请在最新用户消息下方使用编辑按钮。",
            ));
        }
        resolve_last_user_turn_draft(&runtime_state.conversation_state)
    }

    /// Validates shell-level conditions before a last-user rollback can begin.
    /// 在最后用户消息回滚开始前校验外壳层面的条件。
    fn validate_last_user_message_edit_gate(
        &mut self,
        session_id: SessionId,
        require_empty_composer: bool,
    ) -> Result<(), String> {
        if !self.hydrated_session_ids.contains(&session_id) {
            return Err(String::from("当前会话上下文仍在载入，请载入完成后再编辑。"));
        }
        if self.active_subagent_panel.is_some() {
            return Err(String::from(
                "当前正在查看子代理面板，请先返回主会话后再编辑。",
            ));
        }
        if require_empty_composer
            && composer_input_has_submittable_content(
                &self.composer_buffer.text(),
                &self.composer_attachments,
            )
        {
            return Err(String::from(
                "输入框已有内容。为避免覆盖草稿，请先发送、清空或保存当前输入。",
            ));
        }
        if self.session_has_active_run(session_id)
            || self.session_has_active_compaction(session_id)
            || self.session_run_phase_for(session_id) == SessionRunPhase::Preparing
        {
            return Err(String::from(
                "当前会话仍有执行或压缩任务在进行，暂不能回滚历史。",
            ));
        }
        if !self.session_pending_prompt_queue(session_id).is_empty()
            || !self
                .session_pending_prompt_panel_queue(session_id)
                .is_empty()
        {
            return Err(String::from(
                "当前会话仍有待发送队列。请先处理队列，再编辑最后用户消息。",
            ));
        }
        if self.session_has_running_subagent_tasks_after_refresh(session_id) {
            return Err(String::from(
                "当前会话仍有运行中的子代理任务，暂不能回滚历史。",
            ));
        }
        Ok(())
    }

    /// Trims the last user turn from the canonical state and refreshes dependent UI/runtime caches.
    /// 从规范状态中裁剪最后一轮用户输入，并刷新依赖它的界面与运行时缓存。
    fn rollback_last_user_turn_for_edit(
        &mut self,
        session_id: SessionId,
    ) -> Result<LastUserTurnRollback, String> {
        self.validate_last_user_message_edit_gate(session_id, false)?;
        self.ensure_session_conversation_state_loaded(session_id);
        let rollback = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let rollback = rollback_last_user_turn_from_conversation_state(
                &mut runtime_state.conversation_state,
            )?;
            runtime_state.last_provider_usage =
                crate::provider_usage::latest_provider_usage_from_conversation_state(
                    &runtime_state.conversation_state,
                );
            runtime_state.last_successful_provider_usage =
                runtime_state.last_provider_usage.clone();
            runtime_state.live_context_budget_fingerprint = None;
            runtime_state.live_context_assembly_fingerprint = None;
            runtime_state.last_provider_request_preview_markdown = None;
            runtime_state.updated_at = current_unix_millis_text();
            if !runtime_state
                .context_start_message_id
                .as_ref()
                .is_some_and(|message_id| {
                    runtime_state
                        .conversation_state
                        .records
                        .iter()
                        .any(|record| &record.id == message_id)
                })
            {
                runtime_state.context_start_message_id = None;
                runtime_state.last_compaction_id = None;
                runtime_state.anchor_context_budget_fingerprint = None;
                runtime_state.anchor_context_assembly_fingerprint = None;
                self.refresh_vmm_hook_config_snapshot();
            }
            rollback
        };

        self.purge_last_user_rollback_caches(session_id, &rollback.removed_record_ids);
        self.replace_session_messages_from_conversation_state(session_id);
        self.replace_session_runtime_events(session_id, Vec::new());
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.session_last_api_request_buffers.remove(&session_id);
        self.session_last_api_request_markdowns.remove(&session_id);
        self.sync_context_usage_from_session_budget(session_id);
        self.dirty_session_payload_ids.insert(session_id);
        self.dirty_session_message_ids.insert(session_id);
        Ok(rollback)
    }

    /// Removes transcript and projection caches owned by records trimmed during rollback.
    /// 移除回滚期间被裁剪记录拥有的转录与投影缓存。
    fn purge_last_user_rollback_caches(
        &mut self,
        session_id: SessionId,
        removed_record_ids: &[String],
    ) {
        let removed_record_ids = removed_record_ids.iter().collect::<BTreeSet<_>>();
        if let Some(tool_details) = self.session_tool_detail_messages.get_mut(&session_id) {
            tool_details.retain(|message_id, _| !removed_record_ids.contains(message_id));
            if tool_details.is_empty() {
                self.session_tool_detail_messages.remove(&session_id);
            }
        }
        for message_id in &removed_record_ids {
            self.session_message_raw_buffers.remove(*message_id);
            self.session_message_markdown.remove(*message_id);
            self.session_message_markdown_buffers.remove(*message_id);
        }
        self.markdown_session_message_ids.remove(&session_id);
        self.session_agent_view_block_collapsed.remove(&session_id);
        self.session_agent_view_follow_tail.insert(session_id, true);
    }

    /// Replaces the visible transcript projection from the canonical conversation state without preserving removed tails.
    /// 使用规范会话状态替换可见转录投影，且不保留已删除的尾部消息。
    fn replace_session_messages_from_conversation_state(&mut self, session_id: SessionId) {
        let full_projected_messages = self
            .session_runtime_state(session_id)
            .map(|state| conversation_state_to_session_messages(&state.conversation_state))
            .unwrap_or_default();
        self.session_tool_detail_messages.remove(&session_id);
        self.remember_tool_detail_messages(session_id, &full_projected_messages);
        let projected_messages = full_projected_messages
            .iter()
            .map(compact_tool_detail_message_for_projection)
            .collect::<Vec<_>>();
        self.archived_session_messages.remove(&session_id);
        if projected_messages.is_empty() {
            self.session_messages.remove(&session_id);
        } else {
            self.session_messages.insert(session_id, projected_messages);
        }
        self.rebuild_subagent_task_cache_for_session(session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }
}

/// Resolves the last user turn draft without mutating the provided conversation state.
/// 在不修改会话状态的前提下解析最后一轮用户输入草稿。
fn resolve_last_user_turn_draft(
    state: &SessionConversationState,
) -> Result<LastUserMessageEditDraft, String> {
    let last_user_record_index = last_visible_user_record_index(state)
        .ok_or_else(|| String::from("当前会话还没有可编辑的用户消息。"))?;
    ensure_no_successful_compaction_after_record(state, last_user_record_index)?;
    let user_turn_index = visible_user_record_count_before(state, last_user_record_index);
    let user_message = nth_user_protocol_message(state, user_turn_index)
        .ok_or_else(|| String::from("规范上下文缺少最后用户消息，无法安全回滚。"))?;
    let SessionConversationPayload::UserText { text, attachments } = &user_message.payload else {
        return Err(String::from("最后一条规范消息不是用户输入，无法编辑重跑。"));
    };
    Ok(LastUserMessageEditDraft {
        prompt: text.clone(),
        attachments: attachments.clone(),
    })
}

/// Rolls back the final user turn and returns the removed record identifiers.
/// 回滚最后一轮用户输入并返回被移除的记录标识。
fn rollback_last_user_turn_from_conversation_state(
    state: &mut SessionConversationState,
) -> Result<LastUserTurnRollback, String> {
    let draft = resolve_last_user_turn_draft(state)?;
    let last_user_record_index = last_visible_user_record_index(state)
        .ok_or_else(|| String::from("当前会话还没有可编辑的用户消息。"))?;
    let user_turn_index = visible_user_record_count_before(state, last_user_record_index);
    let user_message_index = nth_user_protocol_message_index(state, user_turn_index)
        .ok_or_else(|| String::from("规范上下文缺少最后用户消息，无法安全回滚。"))?;
    let removed_records = state.records.split_off(last_user_record_index);
    let removed_record_ids = removed_records
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    state.messages.truncate(user_message_index);
    state.continuation = Default::default();
    Ok(LastUserTurnRollback {
        draft,
        removed_record_ids,
    })
}

/// Finds the final visible user transcript record in the canonical record stream.
/// 在规范记录流中查找最后一条可见用户转录记录。
fn last_visible_user_record_index(state: &SessionConversationState) -> Option<usize> {
    state.records.iter().rposition(|record| {
        record.kind == SessionMessageKind::User
            && record.record_kind == SessionMessageRecordKind::Transcript
    })
}

/// Counts user transcript records that appear before the provided record index.
/// 统计指定记录下标之前出现的用户转录记录数量。
fn visible_user_record_count_before(
    state: &SessionConversationState,
    record_index: usize,
) -> usize {
    state
        .records
        .iter()
        .take(record_index)
        .filter(|record| {
            record.kind == SessionMessageKind::User
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .count()
}

/// Rejects rollback when a successful compaction checkpoint exists after the user turn.
/// 当用户轮次之后存在成功压缩检查点时拒绝回滚。
fn ensure_no_successful_compaction_after_record(
    state: &SessionConversationState,
    record_index: usize,
) -> Result<(), String> {
    if state
        .records
        .iter()
        .skip(record_index.saturating_add(1))
        .any(|record| record.record_kind == SessionMessageRecordKind::CompactionSummary)
    {
        return Err(String::from(
            "最后用户消息之后已经发生成功的上下文压缩重构，不能回滚该轮历史。",
        ));
    }
    Ok(())
}

/// Returns the nth user protocol message by visible-user turn index.
/// 按可见用户轮次下标返回第 n 条用户协议消息。
fn nth_user_protocol_message(
    state: &SessionConversationState,
    user_turn_index: usize,
) -> Option<&crate::conversation_protocol::SessionConversationMessage> {
    nth_user_protocol_message_index(state, user_turn_index)
        .and_then(|index| state.messages.get(index))
}

/// Returns the message-vector index of the nth user protocol message.
/// 返回第 n 条用户协议消息在消息向量中的下标。
fn nth_user_protocol_message_index(
    state: &SessionConversationState,
    user_turn_index: usize,
) -> Option<usize> {
    state
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| {
            matches!(message.payload, SessionConversationPayload::UserText { .. })
        })
        .nth(user_turn_index)
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one canonical record for focused rollback unit tests.
    /// 为聚焦回滚单元测试构建一条规范记录。
    fn test_record(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
    ) -> SessionConversationRecord {
        SessionConversationRecord {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: String::new(),
            body: id.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }
    }

    /// Builds one canonical user protocol message for focused rollback unit tests.
    /// 为聚焦回滚单元测试构建一条规范用户协议消息。
    fn test_user_message(
        id: &str,
        text: &str,
    ) -> crate::conversation_protocol::SessionConversationMessage {
        crate::conversation_protocol::SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::UserText {
                text: text.to_string(),
                attachments: Vec::new(),
            },
        }
    }

    /// Builds one canonical assistant protocol message for focused rollback unit tests.
    /// 为聚焦回滚单元测试构建一条规范助手协议消息。
    fn test_assistant_message(
        id: &str,
        text: &str,
    ) -> crate::conversation_protocol::SessionConversationMessage {
        crate::conversation_protocol::SessionConversationMessage {
            id: id.to_string(),
            payload: SessionConversationPayload::Assistant {
                text: text.to_string(),
                reasoning: String::new(),
                provider_message_id: Some(format!("provider-{id}")),
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }
    }

    /// Verifies rollback removes only the final user turn and preserves earlier context.
    /// 验证回滚只移除最后用户轮次并保留更早上下文。
    #[test]
    fn rollback_last_user_turn_removes_tail_records_and_messages() {
        let mut state = SessionConversationState::default();
        state.messages = vec![
            test_user_message("u1", "first"),
            test_assistant_message("a1", "one"),
            test_user_message("u2", "second"),
            test_assistant_message("a2", "two"),
        ];
        state.records = vec![
            test_record(
                "r-u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
            ),
            test_record(
                "r-u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-thinking",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ThinkingOutput,
            ),
            test_record(
                "r-tool",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
            ),
            test_record(
                "r-stats",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::TurnStats,
            ),
        ];
        state.continuation.last_provider_message_id = Some(String::from("provider-a2"));

        let rollback = rollback_last_user_turn_from_conversation_state(&mut state)
            .expect("last user rollback should succeed");

        assert_eq!(rollback.draft.prompt, "second");
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.records.len(), 2);
        assert_eq!(
            rollback.removed_record_ids,
            vec!["r-u2", "r-thinking", "r-tool", "r-stats"]
        );
        assert!(state.continuation.last_provider_message_id.is_none());
    }

    /// Verifies rollback is blocked after a successful compaction checkpoint.
    /// 验证成功压缩检查点之后会阻止回滚。
    #[test]
    fn rollback_last_user_turn_rejects_successful_compaction_after_user() {
        let mut state = SessionConversationState::default();
        state.messages = vec![
            test_user_message("u1", "first"),
            test_assistant_message("a1", "one"),
        ];
        state.records = vec![
            test_record(
                "r-u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
            ),
            test_record(
                "r-compact",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::CompactionSummary,
            ),
        ];

        let error = rollback_last_user_turn_from_conversation_state(&mut state)
            .expect_err("successful compaction should block rollback");

        assert!(error.contains("成功的上下文压缩"));
        assert_eq!(state.records.len(), 3);
        assert_eq!(state.messages.len(), 2);
    }

    /// Verifies failed or terminated turns do not count as successful compaction reconstruction.
    /// 验证失败或终止轮次不会被视为成功压缩重构。
    #[test]
    fn rollback_last_user_turn_allows_failed_or_terminated_tail() {
        let mut state = SessionConversationState::default();
        state.messages = vec![test_user_message("u1", "first")];
        state.records = vec![
            test_record(
                "r-u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-error",
                SessionMessageKind::Error,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-term",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::TurnTerminated,
            ),
            test_record(
                "r-stats",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::TurnStats,
            ),
        ];

        let rollback = rollback_last_user_turn_from_conversation_state(&mut state)
            .expect("failed or terminated tail should remain rollbackable");

        assert_eq!(rollback.draft.prompt, "first");
        assert!(state.records.is_empty());
        assert!(state.messages.is_empty());
    }

    /// Verifies an older compaction checkpoint before the final user turn remains editable.
    /// 验证位于最后用户轮次之前的历史压缩检查点不会阻止编辑。
    #[test]
    fn rollback_last_user_turn_allows_prior_compaction_checkpoint() {
        let mut state = SessionConversationState::default();
        state.messages = vec![
            test_assistant_message("summary-1", "summary"),
            test_user_message("u2", "second"),
            test_assistant_message("a2", "two"),
        ];
        state.records = vec![
            test_record(
                "summary-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::CompactionSummary,
            ),
            test_record(
                "r-u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
            ),
            test_record(
                "r-a2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
            ),
        ];

        let rollback = rollback_last_user_turn_from_conversation_state(&mut state)
            .expect("prior compaction should not block the final user rollback");

        assert_eq!(rollback.draft.prompt, "second");
        assert_eq!(state.records.len(), 1);
        assert_eq!(
            state.records[0].record_kind,
            SessionMessageRecordKind::CompactionSummary
        );
        assert_eq!(state.messages.len(), 1);
    }
}

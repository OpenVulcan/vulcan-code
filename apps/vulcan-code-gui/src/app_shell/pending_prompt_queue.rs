//! Pending-prompt queue and dispatch helpers extracted from the main workbench module.
//! 从主工作台模块中抽取的待发送提示词队列与派发辅助实现。

use super::*;

impl VulcanWorkbench {
    /// Returns the persisted pending prompt queue attached to the provided session runtime state.
    /// 返回附着在给定会话运行时状态上的持久化待发送提示词队列。
    pub(super) fn session_pending_prompt_queue(
        &self,
        session_id: SessionId,
    ) -> &[PendingSessionPromptEntry] {
        self.session_runtime_state(session_id)
            .map(|state| state.ui_state.pending_prompt_queue.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the visible prompt-panel queue attached to the provided session runtime state.
    /// 返回附着在给定会话运行时状态上的可见提示词面板队列。
    pub(super) fn session_pending_prompt_panel_queue(
        &self,
        session_id: SessionId,
    ) -> &[PendingPromptPanelEntry] {
        self.session_runtime_state(session_id)
            .map(|state| state.ui_state.pending_prompt_panel_queue.as_slice())
            .unwrap_or(&[])
    }

    /// Returns cloned visible prompt-panel queue entries for the currently selected root session so the floating composer panel can render them safely.
    /// 返回当前选中根会话的可见提示词面板队列副本，供浮动提示词面板安全渲染使用。
    pub(super) fn selected_session_pending_prompt_entries(&self) -> Vec<PendingPromptPanelEntry> {
        self.selected_session()
            .map(|session| self.session_pending_prompt_panel_queue(session.id).to_vec())
            .unwrap_or_default()
    }

    /// Returns whether one parent session still has running subagents that have not fully reported back yet.
    /// 返回某个父会话是否仍有尚未完全回报结果的运行中子代理。
    pub(super) fn session_has_running_subagent_tasks(&self, session_id: SessionId) -> bool {
        self.session_subagent_tasks
            .get(&session_id)
            .is_some_and(|entries| {
                entries.values().any(|state| {
                    self.normalized_subagent_task_panel_state(state)
                        .run_state
                        .is_running()
                })
            })
    }

    /// Returns whether the internal prompt queue still contains delegated subagent completion callbacks for the provided parent session.
    /// 返回给定父会话的内部提示词队列中是否仍包含子代理完成回流回调。
    pub(super) fn session_has_pending_delegate_completion_prompt(
        &self,
        session_id: SessionId,
    ) -> bool {
        self.session_pending_prompt_queue(session_id)
            .iter()
            .any(|entry| entry.source == PendingSessionPromptSource::DelegateCompletion)
    }

    /// Returns whether one session should keep showing its subagent navigation round even after individual child tasks reach terminal states.
    /// 返回某个会话是否仍应保留当前子代理导航轮次，即使单个子任务已经进入终态。
    pub(super) fn session_should_keep_subagent_navigation_round(
        &self,
        session_id: SessionId,
    ) -> bool {
        self.session_has_active_run(session_id)
            || self.session_has_running_subagent_tasks(session_id)
            || self.session_has_pending_delegate_completion_prompt(session_id)
    }

    /// Returns whether the session currently uses the Anthropic runtime family, including third-party providers that are routed through the host-owned Anthropic transport.
    /// 返回该会话当前是否正在使用 Anthropic 运行时族，包含那些被路由到宿主 Anthropic 传输链路的第三方 Provider。
    pub(super) fn session_uses_anthropic_runtime(&self, session_id: SessionId) -> bool {
        self.session_runtime_state(session_id)
            .and_then(|state| state.active_provider_id.as_deref())
            .and_then(provider_config::supported_provider_by_id)
            .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic)
    }

    /// Collapses and hides the subagent navigation round once the parent run, child tasks, and delegated callbacks have all settled.
    /// 当父运行、子任务与委派回调全部收敛后，折叠并隐藏子代理导航轮次。
    pub(super) fn refresh_subagent_navigation_round_visibility(&mut self, session_id: SessionId) {
        if self.session_should_keep_subagent_navigation_round(session_id) {
            return;
        }
        self.session_subagent_navigation_rounds.remove(&session_id);
        if self.selected_session().map(|session| session.id) == Some(session_id) {
            self.subagent_navigation_expanded = false;
        }
    }

    /// Returns whether the internal system execution lane is still busy for the provided session, including active runs, internal queued callbacks, and running subagents.
    /// 返回给定会话的内部系统执行通道当前是否仍然繁忙，包括活动运行、内部排队回调以及运行中的子代理。
    pub(super) fn session_has_blocking_system_prompt_work(&self, session_id: SessionId) -> bool {
        self.session_has_active_run(session_id)
            || (self.session_run_phase_for(session_id) == SessionRunPhase::Preparing
                && !self.session_has_active_run(session_id)
                && self.session_precheck_progress.contains_key(&session_id))
            || self.session_has_active_compaction(session_id)
            || !self.session_pending_prompt_queue(session_id).is_empty()
            || self.session_has_running_subagent_tasks(session_id)
    }

    /// Returns whether the selected root session should currently render the pending-prompt queue panel above the floating composer controls.
    /// 返回当前选中的根会话是否应在浮动提示词控制区上方显示待发送队列面板。
    pub(super) fn should_show_selected_session_pending_prompt_strip(&self) -> bool {
        self.selected_session().is_some()
            && !self.selected_session_pending_prompt_entries().is_empty()
    }

    /// Returns whether the selected session still owns any busy execution lane, including active runs, queued system follow-ups, and running subagents.
    /// 返回当前选中会话是否仍占用任一繁忙执行通道，包括活动运行、系统内部排队跟进与运行中的子代理。
    pub(super) fn selected_session_has_busy_execution_lane(&self) -> bool {
        self.selected_session()
            .map(|session| self.session_has_blocking_system_prompt_work(session.id))
            .unwrap_or(false)
    }

    /// Returns the current floating queue-panel height needed by the composer and transcript spacer calculations.
    /// 返回当前浮动队列面板需要占用的高度，供提示词区与转录留白计算复用。
    pub(super) fn selected_session_pending_prompt_panel_height(&self) -> f32 {
        let entry_count = self.selected_session_pending_prompt_entries().len();
        if entry_count == 0 {
            return 0.0;
        }
        let visible_row_count = entry_count.min(SESSION_PENDING_PROMPT_QUEUE_LIMIT);
        let row_stack_height = visible_row_count as f32 * COMPOSER_PENDING_QUEUE_ROW_HEIGHT;
        let row_gap_height =
            visible_row_count.saturating_sub(1) as f32 * COMPOSER_PENDING_QUEUE_ROW_GAP;
        COMPOSER_PENDING_QUEUE_PANEL_VERTICAL_PADDING * 2.0 + row_stack_height + row_gap_height
    }

    /// Refreshes the lightweight queued-step counter shown on one session card from the persisted prompt queue length.
    /// 根据持久化提示词队列长度刷新单个会话卡片上展示的轻量排队步骤计数。
    pub(super) fn refresh_session_queued_steps(&mut self, session_id: SessionId) {
        let queued_steps = self
            .session_pending_prompt_queue(session_id)
            .len()
            .min(u8::MAX as usize) as u8;
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.queued_steps = queued_steps;
        }
        if let Some(session) = self
            .archived_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.queued_steps = queued_steps;
        }
    }

    /// Refreshes queued-step counters for every visible and archived session after one bulk runtime-state reload.
    /// 在一次批量运行时状态重载后刷新全部可见与归档会话的排队步骤计数。
    pub(super) fn refresh_all_session_queued_steps(&mut self) {
        let session_ids = self
            .sessions
            .iter()
            .map(|session| session.id)
            .chain(self.archived_sessions.iter().map(|session| session.id))
            .collect::<Vec<_>>();
        for session_id in session_ids {
            self.refresh_session_queued_steps(session_id);
        }
    }

    /// Builds one persisted internal pending-prompt entry from the provided prompt, source, and optional delegated task identifier.
    /// 根据给定提示词、来源与可选委派任务标识构建一条持久化内部待发送提示词项。
    pub(super) fn build_pending_prompt_entry(
        &self,
        prompt: String,
        attachments: Vec<ConversationAttachment>,
        source: PendingSessionPromptSource,
        task_id: Option<SessionId>,
        preview: Option<String>,
        panel_entry_id: Option<String>,
    ) -> PendingSessionPromptEntry {
        let created_at = current_unix_millis_text();
        PendingSessionPromptEntry {
            id: format!("pending-prompt-{created_at}-{}", Uuid::new_v4()),
            preview: preview.unwrap_or_else(|| pending_prompt_preview_text(&prompt, &attachments)),
            prompt,
            attachments,
            model_reference: self.current_pending_prompt_model_reference(),
            runtime_options: self.current_pending_prompt_runtime_options(),
            source,
            task_id: task_id.map(|value| value.to_string()),
            panel_entry_id,
            created_at,
        }
    }

    /// Builds one persisted visible prompt-panel entry from the provided user-authored prompt text.
    /// 根据给定用户提示词文本构建一条持久化可见提示词面板项。
    pub(super) fn build_pending_prompt_panel_entry(
        &self,
        prompt: String,
        attachments: Vec<ConversationAttachment>,
    ) -> PendingPromptPanelEntry {
        let created_at = current_unix_millis_text();
        PendingPromptPanelEntry {
            id: format!("pending-panel-{created_at}-{}", Uuid::new_v4()),
            preview: pending_prompt_preview_text(&prompt, &attachments),
            prompt,
            attachments,
            model_reference: self.current_pending_prompt_model_reference(),
            runtime_options: self.current_pending_prompt_runtime_options(),
            state: PendingPromptPanelEntryState::Ready,
            created_at,
        }
    }

    /// Enqueues one pending prompt into the target session, optionally inserting it at the head for interrupt-style dispatch.
    /// 将一条待发送提示词加入目标会话队列，并可选地插入头部以支持插队式调度。
    pub(super) fn enqueue_session_pending_prompt(
        &mut self,
        session_id: SessionId,
        entry: PendingSessionPromptEntry,
        insert_front: bool,
    ) -> Result<(), String> {
        let queue_full_error =
            String::from("待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。");
        let now = current_unix_millis_text();
        let entry_id = entry.id.clone();
        let entry_attachments = entry.attachments.clone();
        let removed_delegate_index = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_queue;

            if entry.source == PendingSessionPromptSource::DelegateCompletion
                && entry.task_id.as_ref().is_some_and(|task_id| {
                    queue.iter().any(|existing| {
                        existing.source == PendingSessionPromptSource::DelegateCompletion
                            && existing.task_id.as_ref() == Some(task_id)
                    })
                })
            {
                return Ok(());
            }

            if queue.len() >= SESSION_PENDING_PROMPT_QUEUE_LIMIT {
                if entry.source == PendingSessionPromptSource::DelegateCompletion {
                    if let Some(index) = queue.iter().position(|existing| {
                        existing.source == PendingSessionPromptSource::DelegateCompletion
                    }) {
                        Some(index)
                    } else {
                        return Err(queue_full_error);
                    }
                } else {
                    return Err(queue_full_error);
                }
            } else {
                None
            }
        };

        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let entry_scope = Self::pending_internal_attachment_ref_scope(session_id, &entry_id);
        self.acquire_staged_attachment_refs(
            &workspace_path,
            &entry_scope,
            &entry_attachments,
            "内部待发送队列",
        )?;

        let removed_for_capacity = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_queue;
            let removed_for_capacity = removed_delegate_index
                .and_then(|index| (index < queue.len()).then(|| queue.remove(index)));

            if insert_front {
                queue.insert(0, entry);
            } else {
                queue.push(entry);
            }
            runtime_state.updated_at = now;
            removed_for_capacity
        };

        if let Some(removed_entry) = removed_for_capacity {
            let removed_scope =
                Self::pending_internal_attachment_ref_scope(session_id, &removed_entry.id);
            self.release_staged_attachment_refs(
                &workspace_path,
                &removed_scope,
                &removed_entry.attachments,
                "内部待发送队列",
            );
        }
        self.refresh_session_queued_steps(session_id);
        Ok(())
    }

    /// Removes and returns the next dispatchable internal prompt for the provided session.
    /// 移除并返回给定会话当前允许分发的下一条内部提示词。
    pub(super) fn dequeue_session_pending_prompt(
        &mut self,
        session_id: SessionId,
    ) -> Option<PendingSessionPromptEntry> {
        let now = current_unix_millis_text();
        let prefer_delegate_completion = self.session_uses_anthropic_runtime(session_id);
        let next_entry = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            if runtime_state.ui_state.pending_prompt_queue.is_empty() {
                None
            } else {
                let dispatch_index = pending_prompt_dispatch_index(
                    runtime_state.ui_state.pending_prompt_queue.as_slice(),
                    prefer_delegate_completion,
                );
                let entry = runtime_state
                    .ui_state
                    .pending_prompt_queue
                    .remove(dispatch_index);
                runtime_state.updated_at = now;
                Some(entry)
            }
        };
        if next_entry.is_some() {
            self.refresh_session_queued_steps(session_id);
        }
        next_entry
    }

    /// Appends one visible prompt-panel entry into the provided session queue.
    /// 向给定会话的可见提示词面板队列追加一条条目。
    pub(super) fn enqueue_session_pending_prompt_panel_entry(
        &mut self,
        session_id: SessionId,
        entry: PendingPromptPanelEntry,
    ) -> Result<(), String> {
        let queue_full_error =
            String::from("待插入消息队列已满，最多保留 10 条，请先处理前面的消息。");
        let now = current_unix_millis_text();
        let entry_id = entry.id.clone();
        let entry_attachments = entry.attachments.clone();
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let scope = Self::pending_panel_attachment_ref_scope(session_id, &entry_id);
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &runtime_state.ui_state.pending_prompt_panel_queue;
            if queue.len() >= SESSION_PENDING_PROMPT_QUEUE_LIMIT {
                return Err(queue_full_error);
            }
        }
        self.acquire_staged_attachment_refs(
            &workspace_path,
            &scope,
            &entry_attachments,
            "可见待发送队列",
        )?;
        let queue_was_full_after_acquire = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_panel_queue;
            if queue.len() >= SESSION_PENDING_PROMPT_QUEUE_LIMIT {
                true
            } else {
                queue.push(entry);
                runtime_state.updated_at = now;
                false
            }
        };
        if queue_was_full_after_acquire {
            self.release_staged_attachment_refs(
                &workspace_path,
                &scope,
                &entry_attachments,
                "可见待发送队列",
            );
            return Err(queue_full_error);
        }
        Ok(())
    }

    /// Returns one cloned visible prompt-panel entry when it still exists in the provided session queue.
    /// 返回给定会话可见提示词面板队列中仍然存在的一条条目副本。
    pub(super) fn pending_prompt_panel_entry(
        &self,
        session_id: SessionId,
        entry_id: &str,
    ) -> Option<PendingPromptPanelEntry> {
        self.session_pending_prompt_panel_queue(session_id)
            .iter()
            .find(|entry| entry.id == entry_id)
            .cloned()
    }

    /// Returns the first visible prompt-panel entry that is still waiting for passive auto-dispatch.
    /// 返回当前仍在等待被动自动分发的第一条可见提示词面板项。
    pub(super) fn first_ready_pending_prompt_panel_entry(
        &self,
        session_id: SessionId,
    ) -> Option<PendingPromptPanelEntry> {
        self.session_pending_prompt_panel_queue(session_id)
            .iter()
            .find(|entry| entry.state == PendingPromptPanelEntryState::Ready)
            .cloned()
    }

    /// Removes one concrete visible prompt-panel entry from the provided session queue and returns it when found.
    /// 从给定会话的可见提示词面板队列中移除一条具体条目，并在找到时返回它。
    pub(super) fn remove_session_pending_prompt_entry(
        &mut self,
        session_id: SessionId,
        entry_id: &str,
    ) -> Option<PendingPromptPanelEntry> {
        let now = current_unix_millis_text();
        let removed_entry = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_panel_queue;
            let index = queue.iter().position(|entry| entry.id == entry_id)?;
            let removed = queue.remove(index);
            runtime_state.updated_at = now;
            Some(removed)
        };
        if let Some(entry) = removed_entry.as_ref() {
            let workspace_path = self.attachment_workspace_path_for_session(session_id);
            let scope = Self::pending_panel_attachment_ref_scope(session_id, &entry.id);
            self.release_staged_attachment_refs(
                &workspace_path,
                &scope,
                &entry.attachments,
                "可见待发送队列",
            );
        }
        removed_entry
    }

    /// Marks one visible prompt-panel entry as already inserted into the internal system queue.
    /// 将某条可见提示词面板条目标记为已经插入到内部系统队列。
    pub(super) fn mark_session_pending_prompt_entry_inserted(
        &mut self,
        session_id: SessionId,
        entry_id: &str,
    ) -> bool {
        let now = current_unix_millis_text();
        let did_update = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_panel_queue;
            let Some(entry) = queue.iter_mut().find(|entry| entry.id == entry_id) else {
                return false;
            };
            if entry.state == PendingPromptPanelEntryState::Inserted {
                return true;
            }
            entry.state = PendingPromptPanelEntryState::Inserted;
            runtime_state.updated_at = now;
            true
        };
        did_update
    }

    /// Restores one visible prompt-panel entry back to the ready state after its linked internal system-queue item gets cleared.
    /// 在关联的内部系统队列项被清除后，将某条可见提示词面板项恢复为可再次插入的待命状态。
    pub(super) fn mark_session_pending_prompt_entry_ready(
        &mut self,
        session_id: SessionId,
        entry_id: &str,
    ) -> bool {
        let now = current_unix_millis_text();
        let did_update = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_panel_queue;
            let Some(entry) = queue.iter_mut().find(|entry| entry.id == entry_id) else {
                return false;
            };
            if entry.state == PendingPromptPanelEntryState::Ready {
                return true;
            }
            entry.state = PendingPromptPanelEntryState::Ready;
            runtime_state.updated_at = now;
            true
        };
        did_update
    }

    /// Clears the internal system prompt queue of one session and optionally restores linked visible panel entries back to the ready state.
    /// 清空某个会话的内部系统提示词队列，并可选地把关联的可见面板项恢复为待命状态。
    pub(super) fn clear_session_pending_prompt_queue(
        &mut self,
        session_id: SessionId,
        restore_panel_entries: bool,
    ) -> bool {
        let now = current_unix_millis_text();
        let cleared_entries = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_queue;
            if queue.is_empty() {
                return false;
            }
            let drained = queue.drain(..).collect::<Vec<_>>();
            runtime_state.updated_at = now;
            drained
        };
        self.refresh_session_queued_steps(session_id);
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        for cleared_entry in &cleared_entries {
            let scope = Self::pending_internal_attachment_ref_scope(session_id, &cleared_entry.id);
            self.release_staged_attachment_refs(
                &workspace_path,
                &scope,
                &cleared_entry.attachments,
                "内部待发送队列",
            );
        }
        if restore_panel_entries {
            for panel_entry_id in cleared_entries
                .iter()
                .filter_map(|entry| entry.panel_entry_id.as_deref())
            {
                let _ = self.mark_session_pending_prompt_entry_ready(session_id, panel_entry_id);
            }
        }
        true
    }

    /// Applies one drag-drop target to reorder the visible prompt-panel entries of the provided session.
    /// 将一次拖拽落点应用到给定会话的可见提示词面板队列，以完成重排。
    pub(super) fn apply_pending_prompt_drop_target(
        &mut self,
        session_id: SessionId,
        dragged_entry_id: &str,
        drop_target: &PendingPromptDropTarget,
    ) -> bool {
        let now = current_unix_millis_text();
        let did_move = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let queue = &mut runtime_state.ui_state.pending_prompt_panel_queue;
            let Some(current_index) = queue.iter().position(|entry| entry.id == dragged_entry_id)
            else {
                return false;
            };
            let mut target_index = match drop_target {
                PendingPromptDropTarget::Before(target_entry_id) => {
                    let Some(index) = queue.iter().position(|entry| entry.id == *target_entry_id)
                    else {
                        return false;
                    };
                    index
                }
                PendingPromptDropTarget::End => queue.len(),
            };
            let entry = queue.remove(current_index);
            if current_index < target_index {
                target_index = target_index.saturating_sub(1);
            }
            if target_index == current_index {
                queue.insert(current_index, entry);
                return false;
            }
            let bounded_target_index = target_index.min(queue.len());
            queue.insert(bounded_target_index, entry);
            runtime_state.updated_at = now;
            true
        };
        did_move
    }

    /// Applies one drag-drop target to reorder the configured route list of the provided semantic AI capability.
    /// 将一次拖拽落点应用到给定语义 AI 能力的已配置路由列表，以完成重排。
    pub(super) fn apply_ai_capability_route_drop_target(
        &mut self,
        capability_kind: AiToolKind,
        dragged_target_selector: &str,
        drop_target: &AiCapabilityRouteDropTarget,
    ) -> bool {
        let before_target = match drop_target {
            AiCapabilityRouteDropTarget::Before(target_selector) => Some(target_selector.as_str()),
            AiCapabilityRouteDropTarget::End => None,
        };
        let did_move = self.ai_tool_configuration.reorder_target_for_kind(
            capability_kind,
            dragged_target_selector,
            before_target,
        );
        if did_move {
            self.persist_ai_tool_configuration_change(format!(
                "已更新“{}”的目标顺序并自动保存。",
                capability_kind.label()
            ));
        }
        did_move
    }

    /// Clears all transient drag state retained for the pending-prompt queue overlay.
    /// 清除为待发送队列覆盖层保留的全部瞬时拖拽状态。
    pub(super) fn clear_pending_prompt_drag_state(&mut self) {
        self.active_pending_prompt_drop_target = None;
        self.pending_pending_prompt_drag = None;
        self.dragging_pending_prompt_entry_id = None;
    }

    /// Marks one session as waiting on already-inserted internal queue work so the UI no longer looks idle while follow-up dispatch is pending.
    /// 将某个会话标记为正在等待已接入的内部队列任务，避免后续分发待命期间界面错误表现为空闲。
    pub(super) fn mark_session_waiting_for_inserted_prompt_dispatch(
        &mut self,
        session_id: SessionId,
    ) {
        if self.session_has_active_run(session_id) {
            return;
        }
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Blocked;
            session.focus =
                String::from("已有消息接入主会话，正在等待当前系统任务与子代理处理完成后继续。");
            session.last_updated_label = String::from("刚刚");
        }
    }

    /// Validates whether the provided prompt entry still matches the active model attachment capability set.
    /// 校验给定提示词项当前是否仍满足激活模型的附件能力集合。
    pub(super) fn validate_prompt_entry_attachments(
        &self,
        prompt_entry: &PendingSessionPromptEntry,
    ) -> Result<(), String> {
        if prompt_entry.attachments.is_empty() {
            return Ok(());
        }
        if prompt_entry.attachments.len() > MAX_COMPOSER_ATTACHMENTS {
            return Err(format!(
                "当前提示词最多支持 {MAX_COMPOSER_ATTACHMENTS} 个附件，请移除多余附件后再发送。"
            ));
        }

        let capabilities = prompt_entry
            .model_reference
            .as_ref()
            .and_then(|model_reference| {
                self.resolve_pending_prompt_model_capabilities(
                    model_reference,
                    &prompt_entry.runtime_options,
                )
            });
        if let Some(capabilities) = capabilities {
            for attachment in &prompt_entry.attachments {
                if !capabilities.allows_staged_attachment_kind(attachment.kind.clone()) {
                    let model_label = prompt_entry
                        .model_reference
                        .as_ref()
                        .map(|model_reference| {
                            if model_reference.model_name.trim().is_empty() {
                                model_reference.model_id.clone()
                            } else {
                                model_reference.model_name.clone()
                            }
                        })
                        .unwrap_or_else(|| String::from("锁定模型"));
                    return Err(format!(
                        "待发送消息锁定的模型 `{model_label}` 未声明支持{}附件 `{}`；请编辑该队列项后重新发送。",
                        attachment.kind.display_label(),
                        attachment.file_name
                    ));
                }
            }
            return Ok(());
        }

        self.validate_staged_attachments(&prompt_entry.attachments)
    }

    /// Inserts one visible prompt-panel entry into the internal system queue immediately, while letting actual execution wait for the system lane to become eligible.
    /// 立即将一条可见提示词面板项接入内部系统队列，同时让真实执行继续等待系统通道变为可运行状态。
    pub(super) fn dispatch_pending_prompt_entry_now(&mut self, entry_id: &str) -> Task<Message> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            self.status_message = String::from("当前没有可执行的会话。");
            return Task::none();
        };
        let Some(panel_entry) = self.pending_prompt_panel_entry(session_id, entry_id) else {
            self.status_message = String::from("未找到可立即处理的待发送消息。");
            return Task::none();
        };
        if panel_entry.state == PendingPromptPanelEntryState::Inserted {
            self.status_message =
                String::from("该消息已经进入系统会话队列，请等待主流程继续处理。");
            return Task::none();
        }
        let mut prompt_entry = self.build_pending_prompt_entry(
            panel_entry.prompt.clone(),
            panel_entry.attachments.clone(),
            PendingSessionPromptSource::UserFollowup,
            None,
            Some(panel_entry.preview.clone()),
            Some(panel_entry.id.clone()),
        );
        prompt_entry.model_reference = panel_entry.model_reference.clone();
        prompt_entry.runtime_options = panel_entry.runtime_options.clone();
        if let Err(error) = self.enqueue_session_pending_prompt(session_id, prompt_entry, true) {
            self.status_message = error;
            return Task::none();
        }
        let _ = self.mark_session_pending_prompt_entry_inserted(session_id, entry_id);
        self.mark_session_waiting_for_inserted_prompt_dispatch(session_id);
        self.status_message = if anthropic_pending_prompt_dispatch_is_blocked(
            self.session_uses_anthropic_runtime(session_id),
            self.session_has_running_subagent_tasks(session_id),
        ) {
            String::from(
                "已接入系统会话队列；Anthropic 协议下需等待当前子代理全部完成后，才能继续插入主会话。",
            )
        } else {
            String::from("已接入系统会话队列，正在尝试立即插入主会话。")
        };
        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_start_next_pending_prompt(session_id, true),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            operation::focus(COMPOSER_EDITOR_ID),
        ])
    }

    /// Restores one queued prompt entry back into the composer so the user can modify it before resubmitting.
    /// 将某条已排队提示词恢复回编辑器，供用户在重新提交前继续修改。
    pub(super) fn edit_pending_prompt_entry(&mut self, entry_id: &str) -> Task<Message> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            self.status_message = String::from("当前没有可编辑队列的会话。");
            return Task::none();
        };
        let Some(prompt_entry) = self.pending_prompt_panel_entry(session_id, entry_id) else {
            self.status_message = String::from("未找到要编辑的待发送消息。");
            return Task::none();
        };
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let edit_scope = format!("pending-edit:{session_id}:{}", prompt_entry.id);
        if let Err(error) = self.acquire_staged_attachment_refs(
            &workspace_path,
            &edit_scope,
            &prompt_entry.attachments,
            "待发送编辑",
        ) {
            self.show_workbench_notice_modal("附件无法编辑", error.clone());
            self.status_message = error;
            return Task::none();
        }
        let restored_prompt = prompt_entry.prompt.clone();
        let restored_attachments = prompt_entry.attachments.clone();
        let restore_task = match self
            .restore_prompt_into_composer(restored_prompt, restored_attachments.clone())
        {
            Ok(task) => task,
            Err(error) => {
                self.release_staged_attachment_refs(
                    &workspace_path,
                    &edit_scope,
                    &restored_attachments,
                    "待发送编辑",
                );
                self.show_workbench_notice_modal("附件无法编辑", error.clone());
                self.status_message = error;
                return Task::none();
            }
        };
        if self
            .remove_session_pending_prompt_entry(session_id, entry_id)
            .is_none()
        {
            self.release_staged_attachment_refs(
                &workspace_path,
                &edit_scope,
                &prompt_entry.attachments,
                "待发送编辑",
            );
            self.status_message = String::from("未找到要编辑的待发送消息。");
            return self.persist_workspace_snapshot_task();
        }
        self.clear_pending_prompt_drag_state();
        self.status_message = String::from("已将待发送消息恢复到输入框，可继续修改。");
        self.release_staged_attachment_refs(
            &workspace_path,
            &edit_scope,
            &restored_attachments,
            "待发送编辑",
        );
        Task::batch([self.persist_workspace_snapshot_task(), restore_task])
    }

    /// Removes one queued prompt entry without dispatching it and persists the updated queue order.
    /// 移除一条已排队提示词且不发送，并持久化更新后的队列顺序。
    pub(super) fn remove_pending_prompt_entry(&mut self, entry_id: &str) -> Task<Message> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            self.status_message = String::from("当前没有可修改队列的会话。");
            return Task::none();
        };
        if self
            .remove_session_pending_prompt_entry(session_id, entry_id)
            .is_none()
        {
            self.status_message = String::from("未找到要移除的待发送消息。");
            return Task::none();
        }
        self.clear_pending_prompt_drag_state();
        self.status_message = String::from("已移除所选待发送消息。");
        self.persist_workspace_snapshot_task()
    }

    /// Starts one concrete runtime run from the provided prompt entry after it has already been accepted for immediate dispatch.
    /// 在某条提示词项已经被接受为立即分发后，根据它启动一次具体运行时执行。
    pub(super) fn begin_session_run_from_prompt_entry(
        &mut self,
        session_id: SessionId,
        session_title: String,
        mut prompt_entry: PendingSessionPromptEntry,
    ) -> Task<Message> {
        if !composer_input_has_submittable_content(&prompt_entry.prompt, &prompt_entry.attachments)
        {
            self.status_message = String::from("请输入任务指令后再启动执行。");
            self.release_prompt_entry_transient_attachment_refs(session_id, &prompt_entry);
            return Task::none();
        }
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let dispatch_scope =
            Self::prompt_dispatch_attachment_ref_scope(session_id, &prompt_entry.id);
        if let Err(error) = self.acquire_staged_attachment_refs(
            &workspace_path,
            &dispatch_scope,
            &prompt_entry.attachments,
            "提示词派发",
        ) {
            if let Some(panel_entry_id) = prompt_entry.panel_entry_id.as_deref() {
                let _ = self.mark_session_pending_prompt_entry_ready(session_id, panel_entry_id);
            }
            self.release_prompt_entry_transient_attachment_refs(session_id, &prompt_entry);
            self.show_workbench_notice_modal("附件无法发送", error.clone());
            self.status_message = error;
            return Task::none();
        }
        if let Err(error) = self.validate_prompt_entry_attachments(&prompt_entry) {
            if let Some(panel_entry_id) = prompt_entry.panel_entry_id.as_deref() {
                let _ = self.mark_session_pending_prompt_entry_ready(session_id, panel_entry_id);
            }
            self.release_prompt_entry_transient_attachment_refs(session_id, &prompt_entry);
            self.show_workbench_notice_modal("附件无法发送", error.clone());
            self.status_message = error;
            return Task::none();
        }
        self.ensure_session_conversation_state_loaded(session_id);
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state.last_safe_provider_request_count = None;
            runtime_state.updated_at = current_unix_millis_text();
        }
        let owning_project_id = self
            .session_by_id(session_id)
            .and_then(|session| session.project_id.clone())
            .filter(|project_id| !project_id.trim().is_empty());
        if prompt_entry.source != PendingSessionPromptSource::DelegateCompletion
            && let (Some(project_id), Some(runtime_state)) = (
                owning_project_id.as_deref(),
                self.session_runtime_state(session_id).cloned(),
            )
            && let Err(error) = crate::workspace_store::snapshot_session_context_before_user_prompt(
                project_id,
                session_id,
                &runtime_state,
            )
        {
            if let Some(panel_entry_id) = prompt_entry.panel_entry_id.as_deref() {
                let _ = self.mark_session_pending_prompt_entry_ready(session_id, panel_entry_id);
            }
            self.release_prompt_entry_transient_attachment_refs(session_id, &prompt_entry);
            self.show_workbench_notice_modal("上下文快照失败", error.clone());
            self.status_message = format!("无法冻结上一轮真实上下文快照：{error}");
            return Task::none();
        }

        let execution_mode = if self.selected_agent_requires_plan_mode() {
            ExecutionMode::Plan
        } else {
            self.execution_mode
        };
        self.execution_mode = execution_mode;
        let prompt_before_injection_sanitizing = prompt_entry.prompt.clone();
        let (injection_ops, sanitized_prompt) =
            crate::agent_runtime::context_injection_runtime::collect_builtin_manual_injection_ops_and_sanitized_prompt(
                &prompt_entry.prompt,
            );
        if sanitized_prompt != prompt_before_injection_sanitizing {
            prompt_entry.prompt = sanitized_prompt;
        }

        if let Err(error) = self.append_user_prompt_to_session_conversation(
            session_id,
            prompt_entry.prompt.clone(),
            prompt_entry.attachments.clone(),
            vec![
                Self::pending_internal_attachment_ref_scope(session_id, &prompt_entry.id),
                Self::prompt_dispatch_attachment_ref_scope(session_id, &prompt_entry.id),
            ],
        ) {
            if let Some(panel_entry_id) = prompt_entry.panel_entry_id.as_deref() {
                let _ = self.mark_session_pending_prompt_entry_ready(session_id, panel_entry_id);
            }
            self.release_prompt_entry_transient_attachment_refs(session_id, &prompt_entry);
            self.show_workbench_notice_modal("附件无法发送", error.clone());
            self.status_message = error;
            return Task::none();
        }
        let hook_conversation_state = self.session_conversation_state(session_id);
        let system_prompt_hook_phase =
            self.vmm_profile_placement_phase_for_next_user_prompt(session_id, true);
        let precheck_request = self.conversation_hook_runtime.prepare_user_prompt_precheck(
            session_id,
            &prompt_entry.id,
            owning_project_id.clone(),
            self.workspace_path_for_session_id(session_id)
                .unwrap_or_else(|| self.workspace_path.clone()),
            prompt_entry.source,
            system_prompt_hook_phase,
            &prompt_entry.prompt,
            &hook_conversation_state,
        );
        let precheck_turn_id = precheck_request.turn_id().to_string();
        if !injection_ops.is_empty() {
            let now = current_unix_millis_text();
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state
                .context_injection_state
                .apply_inserts_for_turn(injection_ops, &now, Some(&precheck_turn_id));
            runtime_state.updated_at = now;
        }
        let effective_vmm_config = {
            let session_layer = self
                .session_runtime_state(session_id)
                .map(|state| &state.vmm);
            let project_layer = self.current_project_vmm_configuration(session_id);
            vulcan_vmm::VmmEffectiveConfig::resolve(
                &self.provider_configuration.vmm,
                Some(&project_layer),
                session_layer,
            )
        };
        let removed_legacy_profile_injection =
            self.remove_legacy_vmm_profile_context_injection_for_session(session_id);
        let vmm_memory_ready = effective_vmm_config.diagnose_memory_flow(true)
            == vulcan_vmm::VmmConfigDiagnostic::Ready;
        let vmm_precheck_enabled = vmm_memory_ready && effective_vmm_config.precheck_enabled;
        let precheck_hook_snapshots = self.conversation_hook_runtime.precheck_hook_snapshots();
        let should_skip_precheck = !vmm_memory_ready
            && !precheck_hook_snapshots.is_empty()
            && precheck_hook_snapshots
                .iter()
                .all(|snapshot| snapshot.registration.id == super::vmm_hooks::VMM_PRECHECK_HOOK_ID);
        let precheck_progress_labels = precheck_hook_snapshots
            .iter()
            .filter(|snapshot| {
                vmm_precheck_enabled
                    || snapshot.registration.id != super::vmm_hooks::VMM_PRECHECK_HOOK_ID
            })
            .filter_map(|snapshot| snapshot.registration.progress_label.clone())
            .map(|label| label.trim().to_string())
            .filter(|label| !label.is_empty())
            .collect::<Vec<_>>();
        if let (Some(project_id), Some(runtime_state)) = (
            owning_project_id.as_deref(),
            self.session_runtime_state(session_id).cloned(),
        ) && let Err(error) = crate::workspace_store::persist_session_context_current_document(
            project_id,
            session_id,
            &runtime_state,
        ) {
            self.push_activity(
                ActivityLevel::Warning,
                "当前上下文持久化失败",
                format!(
                    "会话 `{session_title}` 的当前上下文未能立即写入独立文件，但内存态执行仍会继续：{error}"
                ),
            );
        }
        if removed_legacy_profile_injection {
            self.push_activity(
                ActivityLevel::Success,
                "已清理旧版画像注入",
                format!(
                    "会话 `{session_title}` 的旧版 vmm_profile 隐藏用户注入已移除，画像基点只通过系统提示词 hook 进入请求。"
                ),
            );
        }
        if let Some(panel_entry_id) = prompt_entry.panel_entry_id.as_deref() {
            let _ = self.remove_session_pending_prompt_entry(session_id, panel_entry_id);
            if self.selected_session().map(|session| session.id) == Some(session_id) {
                self.clear_pending_prompt_drag_state();
            }
        }

        if prompt_entry.source != PendingSessionPromptSource::DelegateCompletion {
            self.push_visible_session_record_message_with_metadata(
                session_id,
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "用户指令",
                build_user_prompt_transcript_body(&prompt_entry.prompt, &prompt_entry.attachments),
                build_user_prompt_transcript_metadata(
                    &prompt_entry.prompt,
                    &prompt_entry.attachments,
                ),
                false,
            );
            if composer_text_has_submittable_content(&prompt_entry.prompt)
                && self.session_user_turn_count(session_id) == 1
            {
                let _ = self.maybe_generate_session_title_from_prompt(
                    session_id,
                    prompt_entry.prompt.as_str(),
                );
            }
            self.materialize_session_after_first_user_turn(session_id);
        }
        if should_skip_precheck || precheck_hook_snapshots.is_empty() {
            return self.continue_session_run_after_precheck(
                session_id,
                session_title,
                prompt_entry,
                precheck_request.skip(),
            );
        }
        self.replace_session_runtime_events(session_id, Vec::new());
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
        self.set_session_run_phase(session_id, SessionRunPhase::Preparing);
        self.set_session_precheck_progress(session_id, precheck_progress_labels);
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Running;
            session.last_updated_label = String::from("刚刚");
        }
        let precheck_status_label = self
            .session_precheck_progress
            .get(&session_id)
            .map(SessionPrecheckProgressState::primary_label)
            .unwrap_or("正在执行前置处理");
        self.status_message = format!("{precheck_status_label}，请稍候。");

        let precheck_session_id = session_id;
        let precheck_session_title = session_title.clone();
        let precheck_prompt_entry = prompt_entry.clone();
        let precheck_task = Task::perform(precheck_request.run(), move |outcome| {
            Message::UserPromptPrecheckFinished {
                session_id: precheck_session_id,
                session_title: precheck_session_title.clone(),
                prompt_entry: precheck_prompt_entry.clone(),
                outcome,
            }
        });

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            precheck_task,
        ])
    }

    /// Continues one already-visible prompt after asynchronous precheck hooks have finished.
    /// 在异步 precheck hook 完成后，继续处理一条已显示的提示词。
    pub(super) fn continue_session_run_after_precheck(
        &mut self,
        session_id: SessionId,
        session_title: String,
        mut prompt_entry: PendingSessionPromptEntry,
        outcome: super::conversation_hooks::ConversationPrecheckOutcome,
    ) -> Task<Message> {
        if outcome.session_id != session_id {
            self.clear_session_precheck_progress(outcome.session_id);
            self.clear_session_precheck_progress(session_id);
            self.push_activity(
                ActivityLevel::Warning,
                "Precheck 结果已忽略",
                format!(
                    "收到会话 `{}` 的 precheck 结果，但当前等待的是 `{session_id}`。",
                    outcome.session_id
                ),
            );
            return Task::none();
        }
        self.clear_session_precheck_progress(session_id);
        if self.session_run_phase_for(session_id) == SessionRunPhase::Stopped {
            self.status_message = String::from("记忆召回结束，但当前会话已停止，未继续发起请求。");
            return self.persist_workspace_snapshot_task();
        }

        let mut system_prompt_hook_blocks = Vec::new();
        match outcome.report {
            Ok(report) => {
                if let Some(effect) =
                    super::conversation_hooks::vmm_profile_sync_effect_from_report(&report)
                {
                    let now = current_unix_millis_text();
                    let runtime_state = self.ensure_session_runtime_state_mut(session_id);
                    runtime_state.vmm_profile_sync = effect.state;
                    runtime_state.updated_at = now;
                    self.refresh_vmm_hook_config_snapshot();
                }
                if let Some(rewritten_prompt) = report
                    .payload
                    .value
                    .get("user_content")
                    .and_then(Value::as_str)
                    .filter(|content| *content != prompt_entry.prompt)
                {
                    prompt_entry.prompt = rewritten_prompt.to_string();
                    prompt_entry.preview = pending_prompt_preview_text(
                        &prompt_entry.prompt,
                        &prompt_entry.attachments,
                    );
                    if prompt_entry.source != PendingSessionPromptSource::DelegateCompletion {
                        self.patch_latest_prechecked_user_prompt(
                            session_id,
                            &prompt_entry.prompt,
                            &prompt_entry.attachments,
                        );
                    }
                }
                if !report.injections.is_empty() {
                    let now = current_unix_millis_text();
                    let runtime_state = self.ensure_session_runtime_state_mut(session_id);
                    runtime_state
                        .context_injection_state
                        .apply_inserts_for_turn(report.injections, &now, Some(&outcome.turn_id));
                    runtime_state.updated_at = now;
                }
                system_prompt_hook_blocks = report.system_prompt_blocks;
            }
            Err(error) => {
                self.push_activity(
                    ActivityLevel::Warning,
                    "Precheck hook 执行失败",
                    format!("会话 `{session_title}` 的 precheck 未能完成，已继续真实请求：{error}"),
                );
            }
        }
        let owning_project_id = self
            .session_by_id(session_id)
            .and_then(|session| session.project_id.clone())
            .filter(|project_id| !project_id.trim().is_empty());
        if let (Some(project_id), Some(runtime_state)) = (
            owning_project_id.as_deref(),
            self.session_runtime_state(session_id).cloned(),
        ) && let Err(error) = crate::workspace_store::persist_session_context_current_document(
            project_id,
            session_id,
            &runtime_state,
        ) {
            self.push_activity(
                ActivityLevel::Warning,
                "当前上下文持久化失败",
                format!(
                    "会话 `{session_title}` 的当前上下文未能立即写入独立文件，但内存态执行仍会继续：{error}"
                ),
            );
        }
        let config_for_run = self.provider_configuration_for_prompt_entry(&prompt_entry);
        let runtime_options = prompt_entry.runtime_options;
        let ai_tool_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(&runtime_options);
        let execution_mode = if self.selected_agent_requires_plan_mode() {
            ExecutionMode::Plan
        } else {
            self.execution_mode
        };
        self.execution_mode = execution_mode;

        let request = ExecutionPreparationRequest {
            session_id,
            session_title,
            workspace_path: self
                .workspace_path_for_session_id(session_id)
                .unwrap_or_else(|| self.workspace_path.clone()),
            prompt: prompt_entry.prompt,
            conversation_state: self.session_conversation_state(session_id),
            execution_mode: execution_mode.to_string(),
            temperature: runtime_options.temperature,
            reasoning_effort: runtime_options.reasoning_effort,
            ai_tool_configuration,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: self.current_project_prompt_skills_configuration(session_id),
            session_prompt_skill_overrides: self
                .session_runtime_state(session_id)
                .map(|state| state.prompt_skill_overrides.clone())
                .unwrap_or_default(),
            context_injection: self
                .session_runtime_state(session_id)
                .and_then(|state| {
                    crate::agent_runtime::context_injection_runtime::core_provider_context_injection_from_state_for_turn(
                        &state.context_injection_state,
                        Some(&outcome.turn_id),
                    )
                }),
            system_prompt_hook_blocks: system_prompt_hook_blocks,
            project_vmm: self.current_project_vmm_configuration(session_id),
            session_vmm: self
                .session_runtime_state(session_id)
                .map(|state| state.vmm.clone())
                .unwrap_or_default(),
        };

        self.replace_session_runtime_events(session_id, Vec::new());
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.last_mcp_discovery_rows.clear();
        self.last_mcp_discovery_errors.clear();
        self.clear_session_precheck_progress(session_id);
        self.mark_session_pending_provider_response(session_id);
        self.set_session_run_phase(session_id, SessionRunPhase::Preparing);
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Running;
            session.last_updated_label = String::from("刚刚");
        }
        self.status_message = String::from("正在发起真实执行，请稍候。");

        let run_id = self.next_run_id;
        self.next_run_id = self.next_run_id.saturating_add(1);
        let request_for_run = request.clone();
        let (run_task, run_handle) = Task::run(
            stream_runtime_run(
                config_for_run,
                self.agent_profile_catalog.clone(),
                self.luaskills_runtime.clone(),
                self.mcp_runtime.clone(),
                self.runtime_foundation.task_runtime(),
                self.runtime_foundation.question_runtime(),
                self.runtime_foundation.command_session_runtime(),
                self.runtime_foundation.token_budget_runtime(),
                request_for_run,
            ),
            move |event| Message::RunExecutionStreamEvent { run_id, event },
        )
        .abortable();
        self.active_runs.insert(
            request.session_id,
            ActiveSessionRunState {
                run_id,
                handle: run_handle,
            },
        );

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(request.session_id),
            run_task,
        ])
    }

    /// Rewrites the latest visible user prompt after a transform hook edits the prompt payload.
    /// 当 transform hook 修改提示词载荷后，回写最近一条可见用户提示词。
    fn patch_latest_prechecked_user_prompt(
        &mut self,
        session_id: SessionId,
        prompt: &str,
        attachments: &[ConversationAttachment],
    ) {
        let body = build_user_prompt_transcript_body(prompt, attachments);
        let metadata = build_user_prompt_transcript_metadata(prompt, attachments);
        let updated_at = current_unix_millis_text();
        let patched_record_id = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            if let Some(message) = runtime_state
                .conversation_state
                .messages
                .iter_mut()
                .rev()
                .find(|message| {
                    matches!(message.payload, SessionConversationPayload::UserText { .. })
                })
                && let SessionConversationPayload::UserText { text, .. } = &mut message.payload
            {
                *text = prompt.to_string();
            }

            let patched_record_id = runtime_state
                .conversation_state
                .records
                .iter_mut()
                .rev()
                .find(|record| {
                    record.visible
                        && record.kind == SessionMessageKind::User
                        && record.record_kind == SessionMessageRecordKind::Transcript
                        && record.title == "用户指令"
                })
                .map(|record| {
                    record.body = body.clone();
                    record.metadata = metadata.clone();
                    record.id.clone()
                });
            runtime_state.updated_at = updated_at;
            patched_record_id
        };

        self.sync_session_messages_from_conversation_state(session_id);
        if let Some(message_id) = patched_record_id {
            self.session_message_raw_buffers
                .insert(message_id.clone(), text_editor::Content::with_text(&body));
            let (markdown_content, markdown_buffer) =
                parse_selectable_markdown_body(&body, &self.app_theme);
            self.session_message_markdown
                .insert(message_id.clone(), markdown_content);
            self.session_message_markdown_buffers
                .insert(message_id, markdown_buffer);
        }
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Starts the next queued prompt for one session when the session is idle and auto-dispatch is currently allowed.
    /// 当会话空闲且当前允许自动分发时，为该会话启动队列中的下一条提示词。
    pub(super) fn maybe_start_next_pending_prompt(
        &mut self,
        session_id: SessionId,
        allow_from_stopped_phase: bool,
    ) -> Task<Message> {
        if anthropic_pending_prompt_dispatch_is_blocked(
            self.session_uses_anthropic_runtime(session_id),
            self.session_has_running_subagent_tasks(session_id),
        ) {
            return Task::none();
        }
        if self.session_has_active_run(session_id) {
            return Task::none();
        }
        if self.session_run_phase_for(session_id) == SessionRunPhase::Preparing {
            return Task::none();
        }
        if self.session_has_active_compaction(session_id) {
            return Task::none();
        }
        if !allow_from_stopped_phase
            && self.session_run_phase_for(session_id) == SessionRunPhase::Stopped
        {
            return Task::none();
        }
        let Some(session_title) = self
            .session_by_id(session_id)
            .map(|session| session.title.clone())
        else {
            return Task::none();
        };
        let prompt_entry = if let Some(prompt_entry) =
            self.dequeue_session_pending_prompt(session_id)
        {
            prompt_entry
        } else if let Some(panel_entry) = self.first_ready_pending_prompt_panel_entry(session_id) {
            let mut prompt_entry = self.build_pending_prompt_entry(
                panel_entry.prompt,
                panel_entry.attachments,
                PendingSessionPromptSource::UserFollowup,
                None,
                Some(panel_entry.preview),
                Some(panel_entry.id),
            );
            prompt_entry.model_reference = panel_entry.model_reference;
            prompt_entry.runtime_options = panel_entry.runtime_options;
            prompt_entry
        } else {
            return Task::none();
        };
        self.status_message = match prompt_entry.source {
            PendingSessionPromptSource::DelegateCompletion => {
                String::from("子代理结果已回流，正在继续协调主流程。")
            }
            PendingSessionPromptSource::UserInterrupt => String::from("正在优先处理插队消息。"),
            PendingSessionPromptSource::UserFollowup => {
                if prompt_entry.panel_entry_id.is_some() {
                    String::from("当前轮次已结束，正在自动处理发送队列中的下一条消息。")
                } else {
                    String::from("正在处理排队消息。")
                }
            }
        };
        self.begin_session_run_from_prompt_entry(session_id, session_title, prompt_entry)
    }

    /// Stops every active or queued work item owned by one session, including the root run, child subagents, and pending internal follow-ups.
    /// 停止某个会话拥有的全部活动或排队工作，包括根运行、子代理与内部待处理跟进消息。
    pub(super) fn stop_all_session_work(&mut self, session_id: SessionId) -> Task<Message> {
        if self.session_has_active_run(session_id) {
            return self.stop_session_run_for_session(session_id, false);
        }
        if self.session_has_active_compaction(session_id) {
            return self.stop_session_prompt_compaction_for_session(session_id);
        }
        let had_pending_precheck =
            self.session_run_phase_for(session_id) == SessionRunPhase::Preparing;
        let had_running_subagents = self.session_has_running_subagent_tasks(session_id);
        let cleared_system_queue = self.clear_session_pending_prompt_queue(session_id, true);
        if !had_pending_precheck && !had_running_subagents && !cleared_system_queue {
            self.status_message = String::from("当前没有正在执行的 AI 调用。");
            return Task::none();
        }

        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.clear_session_precheck_progress(session_id);
        self.mark_running_subagent_tasks_stopped(session_id);
        self.set_session_run_phase(session_id, SessionRunPhase::Stopped);
        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Blocked;
            session.last_updated_label = String::from("刚刚");
        }
        self.status_message = String::from("已停止全部 AI 调用与等待中的系统任务。");
        self.push_session_message(
            session_id,
            SessionMessageKind::Assistant,
            "执行已停止",
            "已按用户请求停止当前会话的子代理与系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。",
        );
        let provider_request_count = self.safe_provider_request_count_for_stopped_turn(session_id);
        let terminated_message_id = self.append_session_turn_terminated(
            session_id,
            "The run was stopped manually before a summary could be produced.",
        );
        let _ = self.append_session_turn_stats(
            session_id,
            &terminated_message_id,
            SessionTurnStatsStatus::Stopped,
            provider_request_count,
        );
        self.push_activity(
            ActivityLevel::Warning,
            "会话全部任务已停止",
            "当前会话的根执行、子代理以及系统内部等待中的后续消息都已停止；你可以重新整理待插入列表后再继续。",
        );
        self.refresh_subagent_navigation_round_visibility(session_id);
        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
        ])
    }

    /// Stops one specific session run and optionally continues with the next queued prompt immediately afterward.
    /// 停止某个指定会话的运行，并可选地在其后立即继续处理下一条排队提示词。
    pub(super) fn stop_session_run_for_session(
        &mut self,
        session_id: SessionId,
        continue_with_next_prompt: bool,
    ) -> Task<Message> {
        let Some(active_run) = self.take_active_run_state(session_id) else {
            self.status_message = String::from("当前没有正在执行的 AI 调用。");
            return Task::none();
        };

        active_run.handle.abort();

        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Blocked;
            session.last_updated_label = String::from("刚刚");
        }

        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.clear_session_precheck_progress(session_id);
        self.mark_running_subagent_tasks_stopped(session_id);
        if !continue_with_next_prompt {
            self.clear_session_pending_prompt_queue(session_id, true);
        }
        self.set_session_run_phase(session_id, SessionRunPhase::Stopped);

        if continue_with_next_prompt {
            self.status_message = String::from("已中断当前执行，正在切换到插队消息。");
            self.push_activity(
                ActivityLevel::Warning,
                "AI 调用已插队",
                "当前真实执行已被最新消息打断，系统将立即切换到队列头部的新请求。",
            );
        } else {
            self.status_message = String::from("已停止全部 AI 调用与等待中的系统任务。");
            self.push_session_message(
                session_id,
                SessionMessageKind::Assistant,
                "执行已停止",
                "已按用户请求停止当前 AI 调用、运行中的子代理以及系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。",
            );
            let provider_request_count =
                self.safe_provider_request_count_for_stopped_turn(session_id);
            let terminated_message_id = self.append_session_turn_terminated(
                session_id,
                "The run was stopped manually before a summary could be produced.",
            );
            let _ = self.append_session_turn_stats(
                session_id,
                &terminated_message_id,
                SessionTurnStatsStatus::Stopped,
                provider_request_count,
            );
            self.push_activity(
                ActivityLevel::Warning,
                "会话全部任务已停止",
                "当前真实执行已被用户中止；子代理与等待中的系统消息也已一并停止，可整理待插入列表后重新发起新的调用。",
            );
        }
        self.refresh_subagent_navigation_round_visibility(session_id);

        let next_prompt_task = if continue_with_next_prompt {
            self.maybe_start_next_pending_prompt(session_id, true)
        } else {
            Task::none()
        };
        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            next_prompt_task,
        ])
    }

    /// Stops one specific prompt-compaction task together with queued system work owned by the same session.
    /// 停止某个指定会话拥有的提示词压缩任务及其同属的排队系统工作。
    pub(super) fn stop_session_prompt_compaction_for_session(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        let Some(active_compaction) = self.take_active_compaction_state(session_id) else {
            self.status_message = String::from("当前没有正在执行的上下文压缩任务。");
            return Task::none();
        };

        active_compaction.handle.abort();
        let status_message_id = active_compaction.status_message_id.unwrap_or_else(|| {
            self.push_context_compaction_status_message(session_id, active_compaction.trigger, "")
        });
        let status_body = VulcanWorkbench::build_context_compaction_status_body(
            active_compaction.trigger,
            ContextCompactionStatusPanelState::Stopped,
            "",
            None,
        );
        self.push_visible_session_record_message_with_metadata(
            session_id,
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::CompactionStatus,
            "上下文压缩已停止",
            status_body,
            Some(json!({
                "context_compaction_status": true,
                "context_compaction_section_id": status_message_id,
                "context_compaction_role": "end",
                "context_compaction_state": "stopped",
            })),
            false,
        );
        self.set_session_agent_view_block_collapsed(
            session_id,
            &format!("session-{session_id}-agent-view-context-compaction-{status_message_id}"),
            false,
        );

        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Blocked;
            session.last_updated_label = String::from("刚刚");
        }

        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.clear_session_precheck_progress(session_id);
        self.mark_running_subagent_tasks_stopped(session_id);
        self.clear_session_pending_prompt_queue(session_id, true);
        self.set_session_run_phase(session_id, SessionRunPhase::Stopped);
        self.status_message = String::from("已停止当前会话的上下文压缩任务与等待中的系统消息。");
        self.push_session_message(
            session_id,
            SessionMessageKind::Assistant,
            "压缩已停止",
            "已按用户请求停止当前会话的上下文压缩任务；较新的上下文不会被改写，等待中的系统消息也已一并停止。",
        );
        self.push_activity(
            ActivityLevel::Warning,
            "会话压缩任务已停止",
            "当前会话的提示词式上下文压缩已被用户中止；若仍需要缩短上下文窗口，可稍后再次手动触发压缩。",
        );
        self.refresh_subagent_navigation_round_visibility(session_id);
        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
        ])
    }

    /// Submits the current composer draft for the selected session, staging it in the visible panel whenever the internal system lane is still busy.
    /// 为当前选中会话提交提示词草稿；若内部系统通道仍然繁忙，则先把它暂存到可见面板中。
    pub(super) fn submit_selected_session_prompt(
        &mut self,
        interrupt_running: bool,
    ) -> Task<Message> {
        let Some((session_id, session_title)) = self
            .selected_session()
            .map(|session| (session.id, session.title.clone()))
        else {
            self.status_message = String::from("当前没有可执行的会话。");
            return Task::none();
        };

        let current_prompt = self.composer_buffer.text();
        self.composer = current_prompt.clone();
        if !composer_input_has_submittable_content(&current_prompt, &self.composer_attachments) {
            self.status_message = String::from("请输入任务指令后再启动执行。");
            return Task::none();
        }
        if let Err(error) = self.validate_staged_attachments(&self.composer_attachments) {
            self.show_workbench_notice_modal("附件无法发送", error.clone());
            self.status_message = error;
            return Task::none();
        }

        if !self.hydrated_session_ids.contains(&session_id) {
            self.status_message = String::from("正在加载当前会话上下文，加载完成后请再次发送。");
            return self.schedule_session_hydration(session_id);
        }

        if self.session_has_blocking_system_prompt_work(session_id) {
            let panel_entry = self.build_pending_prompt_panel_entry(
                current_prompt,
                self.composer_attachments.clone(),
            );
            let hook_prompt = panel_entry.prompt.clone();
            let hook_entry_id = panel_entry.id.clone();
            if let Err(error) =
                self.enqueue_session_pending_prompt_panel_entry(session_id, panel_entry)
            {
                self.status_message = error;
                return Task::none();
            }
            self.conversation_hook_runtime.record_queued_user_prompt(
                session_id,
                &hook_entry_id,
                self.session_by_id(session_id)
                    .and_then(|session| session.project_id.clone()),
                self.workspace_path_for_session_id(session_id)
                    .unwrap_or_else(|| self.workspace_path.clone()),
                PendingSessionPromptSource::UserFollowup,
                &hook_prompt,
            );
            self.clear_composer_input();
            self.status_message = String::from(
                "已加入发送队列；当前轮次结束后会自动开始，点击插入可立即接入系统队列并插队。",
            );
            return Task::batch([
                self.persist_workspace_snapshot_task(),
                operation::focus(COMPOSER_EDITOR_ID),
            ]);
        }

        let prompt_source = if interrupt_running {
            PendingSessionPromptSource::UserInterrupt
        } else {
            PendingSessionPromptSource::UserFollowup
        };
        let prompt_entry = self.build_pending_prompt_entry(
            current_prompt,
            self.composer_attachments.clone(),
            prompt_source,
            None,
            None,
            None,
        );
        if interrupt_running && self.session_has_active_run(session_id) {
            self.conversation_hook_runtime.record_queued_user_prompt(
                session_id,
                &prompt_entry.id,
                self.session_by_id(session_id)
                    .and_then(|session| session.project_id.clone()),
                self.workspace_path_for_session_id(session_id)
                    .unwrap_or_else(|| self.workspace_path.clone()),
                prompt_source,
                &prompt_entry.prompt,
            );
        }
        let workspace_path = self.attachment_workspace_path_for_session(session_id);
        let dispatch_scope =
            Self::prompt_dispatch_attachment_ref_scope(session_id, &prompt_entry.id);
        if let Err(error) = self.acquire_staged_attachment_refs(
            &workspace_path,
            &dispatch_scope,
            &prompt_entry.attachments,
            "提示词派发",
        ) {
            self.show_workbench_notice_modal("附件无法发送", error.clone());
            self.status_message = error;
            return Task::none();
        }
        self.clear_composer_input();
        self.begin_session_run_from_prompt_entry(session_id, session_title, prompt_entry)
    }

    /// Enqueues one delegated subagent completion follow-up so the parent coordinator can absorb the result on the next available turn.
    /// 将一条子代理完成后的跟进消息排入队列，以便父协调代理在下一次可用回合吸收该结果。
    pub(super) fn enqueue_delegate_completion_followup_if_needed(
        &mut self,
        parent_session_id: SessionId,
        state: &SubagentTaskPanelState,
    ) {
        if self.session_run_phase_for(parent_session_id) == SessionRunPhase::Stopped {
            return;
        }
        let prompt_entry = self.build_pending_prompt_entry(
            build_delegate_completion_followup_prompt(state),
            Vec::new(),
            PendingSessionPromptSource::DelegateCompletion,
            Some(state.task_id),
            Some(build_delegate_completion_followup_preview(state)),
            None,
        );
        if let Err(error) =
            self.enqueue_session_pending_prompt(parent_session_id, prompt_entry, false)
        {
            self.push_activity(
                ActivityLevel::Warning,
                "子代理回流排队失败",
                format!(
                    "子代理 `{}` 的结果未能加入后续处理队列：{error}",
                    state.subagent_label
                ),
            );
        }
    }
}

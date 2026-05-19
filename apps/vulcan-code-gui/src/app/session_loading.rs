//! Session-loading methods extracted from the main workbench module.
//! 从主工作台模块中抽取的会话加载方法。

use super::*;

impl VulcanWorkbench {
    /// Schedules the first lazy hydration task for one session when it has not been loaded yet.
    /// 在单个会话尚未加载时，调度首次懒加载水合任务。
    pub(super) fn schedule_session_hydration(&mut self, session_id: SessionId) -> Task<Message> {
        if session_id.is_nil()
            || self.hydrated_session_ids.contains(&session_id)
            || !self.loading_session_ids.insert(session_id)
        {
            return Task::none();
        }

        let Some(controller_runtime) = self.controller_runtime.clone() else {
            self.loading_session_ids.remove(&session_id);
            return Task::none();
        };

        self.status_message = String::from("正在按需加载当前会话正文。");
        Task::perform(
            async move {
                let result = workspace_store::load_session_hydration_snapshot(
                    controller_runtime,
                    session_id,
                    SESSION_TRANSCRIPT_WINDOW_LIMIT,
                )
                .await;
                (session_id, result)
            },
            |(session_id, result)| Message::SessionHydrationLoaded(session_id, result),
        )
    }

    /// Schedules one older transcript page load when the current session window reaches the top.
    /// 当当前会话窗口滚动到顶部时，调度一页更早转录历史加载。
    pub(super) fn schedule_older_session_history_load(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        if session_id.is_nil()
            || !self.session_history_has_more.contains(&session_id)
            || !self.loading_older_session_history_ids.insert(session_id)
        {
            return Task::none();
        }

        let Some(before_ordinal) = self
            .session_history_earliest_ordinals
            .get(&session_id)
            .copied()
        else {
            self.loading_older_session_history_ids.remove(&session_id);
            return Task::none();
        };
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            self.loading_older_session_history_ids.remove(&session_id);
            return Task::none();
        };

        Task::perform(
            async move {
                let result = workspace_store::load_session_history_page(
                    controller_runtime,
                    session_id,
                    before_ordinal,
                    SESSION_TRANSCRIPT_WINDOW_LIMIT,
                )
                .await;
                (session_id, result)
            },
            |(session_id, result)| Message::OlderSessionHistoryLoaded(session_id, result),
        )
    }

    /// Schedules one tool-detail page load for a specific conversation turn.
    /// 为指定对话轮次调度一页工具明细加载。
    pub(super) fn schedule_session_tool_detail_load(
        &mut self,
        session_id: SessionId,
        turn_index: i64,
    ) -> Task<Message> {
        if session_id.is_nil() {
            return Task::none();
        }
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };

        Task::perform(
            async move {
                let result = workspace_store::load_session_tool_detail_page(
                    controller_runtime,
                    session_id,
                    turn_index,
                    None,
                    SESSION_TOOL_DETAIL_PAGE_LIMIT,
                )
                .await;
                (session_id, result)
            },
            move |(session_id, result)| {
                Message::SessionToolDetailsLoaded(session_id, turn_index, result)
            },
        )
    }

    /// Schedules tool-detail loads for every relevant turn already present in one hydrated transcript window.
    /// 为单个已水合转录窗口中所有相关轮次统一调度工具明细加载任务。
    pub(super) fn schedule_session_tool_detail_loads(
        &mut self,
        session_id: SessionId,
        turn_indices: &[i64],
    ) -> Task<Message> {
        let unique_turn_indices = turn_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if unique_turn_indices.is_empty() {
            return Task::none();
        }

        Task::batch(
            unique_turn_indices
                .into_iter()
                .map(|turn_index| self.schedule_session_tool_detail_load(session_id, turn_index)),
        )
    }

    /// Returns whether the selected session should delay its very first transcript projection until the first batch of required tool-detail pages has arrived.
    /// 返回当前已选会话是否应延迟首次转录投影，直到首批必要工具明细分页全部到齐。
    pub(super) fn should_defer_initial_selected_session_projection(
        &self,
        session_id: SessionId,
        turn_indices: &[i64],
    ) -> bool {
        self.selected_session()
            .is_some_and(|session| session.id == session_id)
            && !turn_indices.is_empty()
            && match self.session_agent_view_documents.get(&session_id) {
                Some(document) => document.blocks.is_empty(),
                None => true,
            }
    }

    /// Starts one deferred first-render tool-detail batch so the selected transcript waits for required detail rows before the first heavy projection.
    /// 启动一组延迟首屏工具明细批次，使已选转录在首次重投影前等待必要明细行到齐。
    pub(super) fn begin_session_initial_tool_detail_batch(
        &mut self,
        session_id: SessionId,
        turn_indices: &[i64],
    ) {
        let total_turn_count = turn_indices.iter().copied().collect::<BTreeSet<_>>().len();
        if total_turn_count == 0 {
            self.session_initial_tool_detail_batches.remove(&session_id);
            return;
        }

        self.session_initial_tool_detail_batches.insert(
            session_id,
            SessionInitialToolDetailBatchState {
                total_turn_count,
                completed_turn_indices: BTreeSet::new(),
                failed_turn_count: 0,
            },
        );
    }

    /// Marks one deferred first-render tool-detail turn as finished and returns the completed batch snapshot once every required turn has completed.
    /// 标记一项延迟首屏工具明细轮次已经结束，并在全部必要轮次完成后返回整批完成快照。
    pub(super) fn finish_session_initial_tool_detail_turn(
        &mut self,
        session_id: SessionId,
        turn_index: i64,
        succeeded: bool,
    ) -> Option<SessionInitialToolDetailBatchState> {
        let Some(batch_state) = self.session_initial_tool_detail_batches.get_mut(&session_id) else {
            return None;
        };

        let inserted = batch_state.completed_turn_indices.insert(turn_index);
        if inserted && !succeeded {
            batch_state.failed_turn_count += 1;
        }

        if batch_state.completed_turn_count() >= batch_state.total_turn_count {
            return self.session_initial_tool_detail_batches.remove(&session_id);
        }

        None
    }

    /// Applies one lazily loaded session payload to live transcript and runtime caches.
    /// 将一份懒加载出的会话载荷应用到实时转录与运行时缓存。
    pub(super) fn apply_session_hydration_snapshot(&mut self, snapshot: SessionHydrationSnapshot) {
        let session_id = snapshot.session_id;
        self.hydrated_session_ids.insert(session_id);
        self.loading_session_ids.remove(&session_id);
        if let Some(earliest_ordinal) = snapshot.earliest_ordinal {
            self.session_history_earliest_ordinals
                .insert(session_id, earliest_ordinal);
        } else {
            self.session_history_earliest_ordinals.remove(&session_id);
        }
        if snapshot.has_more_history {
            self.session_history_has_more.insert(session_id);
        } else {
            self.session_history_has_more.remove(&session_id);
        }

        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        self.remember_tool_detail_messages(session_id, &snapshot.messages);
        let projected_messages = snapshot
            .messages
            .iter()
            .map(compact_tool_detail_message_for_projection)
            .collect::<Vec<_>>();
        if session_is_archived {
            self.archived_session_messages
                .insert(session_id, projected_messages);
            self.archived_session_compactions
                .insert(session_id, snapshot.compactions);
            if let Some(runtime_state) = snapshot.runtime_state {
                self.archived_session_runtime_state
                    .insert(session_id, runtime_state);
            }
        } else {
            self.session_messages.insert(session_id, projected_messages);
            self.session_compactions
                .insert(session_id, snapshot.compactions);
            if let Some(runtime_state) = snapshot.runtime_state {
                self.session_runtime_state.insert(session_id, runtime_state);
            }
        }

        self.rebuild_subagent_task_cache_for_session(session_id);
        self.clear_session_agent_view_text_buffers(session_id);
        self.session_agent_view_documents.remove(&session_id);
        self.session_agent_view_block_collapsed.remove(&session_id);
        self.refresh_session_queued_steps(session_id);
    }

    /// Prepends one older transcript page to an already hydrated session cache.
    /// 将一页更早转录历史追加到已水合会话缓存的头部。
    pub(super) fn apply_older_session_history_page(&mut self, page: SessionHistoryPage) {
        let session_id = page.session_id;
        let page_messages = page.messages;
        if page_messages.is_empty() {
            self.session_history_has_more.remove(&session_id);
            return;
        }
        self.remember_tool_detail_messages(session_id, &page_messages);

        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let messages = if session_is_archived {
            self.archived_session_messages
                .entry(session_id)
                .or_default()
        } else {
            self.session_messages.entry(session_id).or_default()
        };
        let existing_ids = messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<BTreeSet<_>>();
        let mut older_messages = page_messages
            .into_iter()
            .filter(|message| !existing_ids.contains(&message.id))
            .map(|message| compact_tool_detail_message_for_projection(&message))
            .collect::<Vec<_>>();
        older_messages.append(messages);
        *messages = older_messages;

        if let Some(earliest_ordinal) = page.earliest_ordinal {
            self.session_history_earliest_ordinals
                .insert(session_id, earliest_ordinal);
        }
        if page.has_more_history {
            self.session_history_has_more.insert(session_id);
        } else {
            self.session_history_has_more.remove(&session_id);
        }

        let removed_count = self.prune_session_message_cache_to_limit(session_id);
        if removed_count > 0 {
            self.status_message = format!(
                "已加载更早历史，并为控制内存保留最近 {SESSION_IN_MEMORY_MESSAGE_LIMIT} 条消息。"
            );
        }
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Merges one on-demand tool-detail page into the already hydrated transcript cache.
    /// 将一页按需工具明细合并到已经水合的转录缓存中。
    pub(super) fn apply_session_tool_detail_page(
        &mut self,
        page: SessionToolDetailPage,
        refresh_agent_view_buffers: bool,
    ) {
        let session_id = page.session_id;
        let _detail_cursor = (page.turn_index, page.earliest_ordinal, page.has_more_detail);
        let page_messages = page.messages;
        if page_messages.is_empty() {
            return;
        }
        self.remember_tool_detail_messages(session_id, &page_messages);

        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let messages = if session_is_archived {
            self.archived_session_messages
                .entry(session_id)
                .or_default()
        } else {
            self.session_messages.entry(session_id).or_default()
        };
        let existing_ids = messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<BTreeSet<_>>();
        messages.extend(
            page_messages
                .into_iter()
                .filter(|message| !existing_ids.contains(&message.id))
                .map(|message| compact_tool_detail_message_for_projection(&message)),
        );
        messages
            .sort_by_key(|message| parse_unix_millis_text(&message.created_at).unwrap_or_default());

        self.prune_session_message_cache_to_limit(session_id);
        if refresh_agent_view_buffers {
            self.refresh_session_agent_view_buffers(session_id);
        }
    }
}

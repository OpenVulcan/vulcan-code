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
            |(session_id, result)| Message::SessionToolDetailsLoaded(session_id, result),
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
        if session_is_archived {
            self.archived_session_messages
                .insert(session_id, snapshot.messages);
            self.archived_session_compactions
                .insert(session_id, snapshot.compactions);
            if let Some(runtime_state) = snapshot.runtime_state {
                self.archived_session_runtime_state
                    .insert(session_id, runtime_state);
            }
        } else {
            self.session_messages.insert(session_id, snapshot.messages);
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
        if page.messages.is_empty() {
            self.session_history_has_more.remove(&session_id);
            return;
        }

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
        let mut older_messages = page
            .messages
            .into_iter()
            .filter(|message| !existing_ids.contains(&message.id))
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
    pub(super) fn apply_session_tool_detail_page(&mut self, page: SessionToolDetailPage) {
        let session_id = page.session_id;
        let _detail_cursor = (page.turn_index, page.earliest_ordinal, page.has_more_detail);
        if page.messages.is_empty() {
            return;
        }

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
            page.messages
                .into_iter()
                .filter(|message| !existing_ids.contains(&message.id)),
        );
        messages
            .sort_by_key(|message| parse_unix_millis_text(&message.created_at).unwrap_or_default());

        self.prune_session_message_cache_to_limit(session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }
}

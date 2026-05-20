//! Session-projection methods extracted from the main workbench module.
//! 从主工作台模块中抽取的会话投影方法。

use super::*;

/// Rewrites one inline transcript block in place so tool-card bodies stay modal-only while the visible main transcript keeps the same block identities and metadata.
/// 原地改写单个内联转录区块，使工具卡正文只保留给弹层使用，同时让主转录继续沿用相同的区块身份与元数据。
fn prepare_session_inline_render_block(block: &mut vulcan_agent_view::AgentViewBlock) {
    let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
        return;
    };
    if !matches!(
        directive.kind,
        vulcan_agent_view::AgentViewDirectiveKind::ToolCall
            | vulcan_agent_view::AgentViewDirectiveKind::ToolResult
    ) {
        return;
    }

    let raw_source =
        vulcan_agent_view::render_directive_source(&directive.kind, &directive.attributes, "");
    *block = vulcan_agent_view::AgentViewBlock::Directive(
        vulcan_agent_view::AgentViewDirectiveBlock::new(
            directive.kind.clone(),
            directive.attributes.clone(),
            "",
            raw_source,
            directive.collapsed_by_default,
        ),
    );
}

/// Decorates and compacts the inline transcript suffix starting at the provided index so streaming updates refresh only the changed tail plus any suffix blocks whose stable GUI identifiers depend on block positions.
/// 从给定索引开始装饰并压缩内联转录后缀，使流式更新只刷新变化尾部以及稳定图形界面标识依赖区块位置的后缀区块。
fn decorate_and_prepare_inline_document_suffix(
    workbench: &VulcanWorkbench,
    session_id: SessionId,
    document: &mut vulcan_agent_view::AgentViewDocument,
    start_index: usize,
) {
    for block_index in start_index..document.blocks.len() {
        let block = &mut document.blocks[block_index];
        let block_id = agent_view_block_id(session_id, block_index, block);
        workbench.decorate_agent_view_block_for_render(session_id, &block_id, block);
        prepare_session_inline_render_block(block);
    }
}

impl VulcanWorkbench {
    /// Returns whether one session already owns a complete cached transcript projection for the currently active transcript surface mode.
    /// 返回某个会话当前是否已经拥有与活动转录表面模式匹配的一整套缓存转录投影。
    pub(super) fn session_agent_view_projection_ready(&self, session_id: SessionId) -> bool {
        if !self.session_agent_view_documents.contains_key(&session_id)
            || !self
                .session_inline_agent_view_documents
                .contains_key(&session_id)
        {
            return false;
        }

        if self.should_materialize_session_agent_view_text_buffers(session_id) {
            self.session_agent_view_selectable_documents
                .contains_key(&session_id)
                && self.session_agent_view_buffers.contains_key(&session_id)
                && self
                    .session_agent_view_raw_buffers
                    .contains_key(&session_id)
        } else {
            true
        }
    }

    /// Schedules one background refresh that materializes the unified transcript document and optional selectable/raw projections for a hydrated session.
    /// 为单个已水合会话调度一条后台刷新任务，用于物化统一转录文档及可选的可选文本/原文投影。
    pub(super) fn schedule_session_agent_view_projection_refresh(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        if session_id.is_nil()
            || !self
                .loading_session_agent_view_projection_ids
                .insert(session_id)
        {
            return Task::none();
        }

        let floating_thinking_message_id = if self.session_has_active_run(session_id)
            && self
                .floating_streaming_thinking_sessions
                .contains(&session_id)
        {
            self.streaming_thinking_message_ids
                .get(&session_id)
                .cloned()
        } else {
            None
        };
        let raw_messages = self
            .session_messages
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let active_streaming_output_message_id = self
            .streaming_output_message_ids
            .get(&session_id)
            .cloned();
        let runtime_events = self
            .session_runtime_events
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let locale = self.current_locale();

        Task::perform(
            async move {
                let task = tokio::task::spawn_blocking(move || {
                    let visible_messages = visible_session_messages_for_agent_view_projection(
                        raw_messages,
                        floating_thinking_message_id.as_deref(),
                    );
                    build_session_agent_view_projection_snapshot(
                        session_id,
                        visible_messages,
                        runtime_events,
                        active_streaming_output_message_id,
                        locale,
                    )
                });
                task.await
                    .map_err(|error| format!("后台构建会话视图失败：{error}"))
            },
            move |result| Message::SessionAgentViewProjectionLoaded(session_id, result),
        )
    }

    /// Schedules one background transcript projection refresh only when the selected session is already hydrated but the current surface still misses required caches.
    /// 仅当已选会话已经完成水合、但当前表面仍缺失必要缓存时，才调度一条后台转录投影刷新任务。
    pub(super) fn schedule_session_agent_view_projection_refresh_if_needed(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        if session_id.is_nil()
            || !self.hydrated_session_ids.contains(&session_id)
            || self.session_agent_view_projection_ready(session_id)
        {
            return Task::none();
        }

        self.schedule_session_agent_view_projection_refresh(session_id)
    }

    /// Clears per-message projection caches for messages that are no longer retained in memory.
    /// 清理已经不再保留在内存中的消息对应的逐消息投影缓存。
    pub(super) fn clear_session_message_view_caches(&mut self, message_ids: &[String]) {
        for message_id in message_ids {
            self.session_message_raw_buffers.remove(message_id);
            self.session_message_markdown.remove(message_id);
            self.session_message_markdown_buffers.remove(message_id);
        }
    }

    /// Returns whether one session currently needs fully materialized selectable/raw transcript buffers.
    /// 返回某个会话当前是否确实需要完整物化的可选结构化/原文转录缓冲区。
    pub(super) fn should_materialize_session_agent_view_text_buffers(
        &self,
        session_id: SessionId,
    ) -> bool {
        self.selected_session()
            .is_some_and(|session| session.id == session_id)
            && self.agent_transcript_surface_mode != AgentTranscriptSurfaceMode::DetailCards
    }

    /// Clears the heavyweight selectable/raw transcript projections while keeping the structural document cache intact.
    /// 清理较重的可选结构化/原文转录投影，同时保留结构化文档缓存本身。
    pub(super) fn clear_session_agent_view_text_buffers(&mut self, session_id: SessionId) {
        self.session_agent_view_buffers.remove(&session_id);
        self.session_agent_view_raw_buffers.remove(&session_id);
        self.session_agent_view_selectable_documents
            .remove(&session_id);
    }

    /// Clears the cached prepared inline transcript snapshot for one session when the main rendered surface can no longer reuse it.
    /// 当主渲染表面不再能复用某会话的内联转录快照时，清空其缓存。
    pub(super) fn clear_session_inline_agent_view_document_cache(&mut self, session_id: SessionId) {
        self.session_inline_agent_view_documents.remove(&session_id);
    }

    /// Rebuilds the prepared inline transcript snapshot reused by the main rendered surface so normal GUI view passes do not reclone and recompact the entire structural document.
    /// 重建主渲染表面复用的内联转录快照，避免普通界面刷新时重新克隆并重新压缩整份结构化文档。
    pub(super) fn refresh_session_inline_agent_view_document_cache(
        &mut self,
        session_id: SessionId,
    ) {
        let Some(mut document) = self.session_agent_view_documents.get(&session_id).cloned() else {
            self.clear_session_inline_agent_view_document_cache(session_id);
            return;
        };

        decorate_and_prepare_inline_document_suffix(self, session_id, &mut document, 0);
        self.session_inline_agent_view_documents.insert(
            session_id,
            SessionInlineAgentViewDocumentCache {
                document: Rc::new(document),
            },
        );
    }

    /// Refreshes only the cached inline transcript range owned by one changed message, then reapplies lightweight runtime decorations to the remaining suffix whose block positions may have shifted.
    /// 仅刷新单条变更消息拥有的缓存内联转录范围，并对区块位置可能发生偏移的剩余后缀重新附着轻量运行时装饰。
    pub(super) fn refresh_session_inline_agent_view_message_blocks(
        &mut self,
        session_id: SessionId,
        start_index: usize,
        replaced_block_count: usize,
        replacement_blocks: &[vulcan_agent_view::AgentViewBlock],
    ) {
        let mut perf_trace = TranscriptPerfTrace::begin(
            "refresh_session_inline_agent_view_message_blocks",
            format!(
                "session_id={session_id} start_index={start_index} replaced_block_count={replaced_block_count} replacement_block_count={}",
                replacement_blocks.len()
            ),
        );
        let mut phase_started_at = Instant::now();
        let Some(mut cache) = self.session_inline_agent_view_documents.remove(&session_id) else {
            perf_trace.checkpoint("take_inline_cache", phase_started_at);
            self.refresh_session_inline_agent_view_document_cache(session_id);
            perf_trace.finish();
            return;
        };
        perf_trace.checkpoint("take_inline_cache", phase_started_at);

        phase_started_at = Instant::now();
        let document = Rc::make_mut(&mut cache.document);
        let end_index = start_index.saturating_add(replaced_block_count);
        if end_index > document.blocks.len() {
            perf_trace.checkpoint("splice_bounds_validation", phase_started_at);
            self.refresh_session_inline_agent_view_document_cache(session_id);
            perf_trace.finish();
            return;
        }
        perf_trace.checkpoint("splice_bounds_validation", phase_started_at);

        phase_started_at = Instant::now();
        document
            .blocks
            .splice(start_index..end_index, replacement_blocks.iter().cloned());
        perf_trace.checkpoint("splice_inline_blocks", phase_started_at);
        phase_started_at = Instant::now();
        decorate_and_prepare_inline_document_suffix(self, session_id, document, start_index);
        perf_trace.checkpoint("decorate_inline_suffix", phase_started_at);
        phase_started_at = Instant::now();
        self.session_inline_agent_view_documents
            .insert(session_id, cache);
        perf_trace.checkpoint("store_inline_cache", phase_started_at);
        perf_trace.finish();
    }

    /// Stores one session-scoped last-provider-request preview buffer for the dedicated request tab.
    /// 为专用请求页签保存一份按会话归属的最后一次 Provider 请求预览缓冲区。
    pub(super) fn replace_session_last_api_request_preview(
        &mut self,
        session_id: SessionId,
        preview_markdown: String,
    ) {
        self.session_last_api_request_buffers.insert(
            session_id,
            text_editor::Content::with_text(&preview_markdown),
        );
    }

    /// Applies one background-built session-scoped transcript projection snapshot to the live workbench caches.
    /// 将一份后台构建完成的按会话归属转录投影视图快照应用到实时工作台缓存。
    pub(super) fn apply_session_agent_view_projection_snapshot(
        &mut self,
        snapshot: SessionAgentViewProjectionSnapshot,
    ) {
        let session_id = snapshot.session_id;

        if snapshot.document.blocks.is_empty() {
            self.clear_session_agent_view_text_buffers(session_id);
            self.session_agent_view_documents.remove(&session_id);
            self.clear_session_inline_agent_view_document_cache(session_id);
            self.session_agent_view_block_collapsed.remove(&session_id);
            self.session_agent_view_follow_tail.remove(&session_id);
            self.refresh_active_modal_payloads_for_session(session_id);
            return;
        }

        self.sync_session_agent_view_block_collapsed_state(session_id, &snapshot.document);
        self.session_agent_view_documents
            .insert(session_id, snapshot.document);
        self.refresh_session_inline_agent_view_document_cache(session_id);

        if self.should_materialize_session_agent_view_text_buffers(session_id) {
            self.session_agent_view_buffers.insert(
                session_id,
                text_editor::Content::with_text(&snapshot.selectable_text),
            );
            self.session_agent_view_raw_buffers.insert(
                session_id,
                text_editor::Content::with_text(&snapshot.raw_source_text),
            );
            self.session_agent_view_selectable_documents
                .insert(session_id, snapshot.selectable_document);
        } else {
            self.clear_session_agent_view_text_buffers(session_id);
        }

        self.refresh_active_modal_payloads_for_session(session_id);
    }

    /// Rebuilds the structural agent-view document cache for one session without forcing every text projection to refresh.
    /// 为单个会话重建结构化 agent-view 文档缓存，而不强制刷新全部文本投影。
    pub(super) fn refresh_session_agent_view_document_cache(&mut self, session_id: SessionId) {
        let messages = self.visible_session_messages_for_agent_view(session_id);
        let runtime_events = self
            .session_runtime_events
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let build_options = agent_view_build_options_for_streaming_output(
            self.streaming_output_message_ids
                .get(&session_id)
                .map(String::as_str),
            self.current_locale(),
        );
        let document =
            build_agent_view_document_with_options(&messages, &runtime_events, &build_options);

        if document.blocks.is_empty() {
            self.clear_session_agent_view_text_buffers(session_id);
            self.session_agent_view_documents.remove(&session_id);
            self.clear_session_inline_agent_view_document_cache(session_id);
            self.session_agent_view_block_collapsed.remove(&session_id);
            self.session_agent_view_follow_tail.remove(&session_id);
            return;
        }

        self.sync_session_agent_view_block_collapsed_state(session_id, &document);
        self.session_agent_view_documents
            .insert(session_id, document);
        self.refresh_session_inline_agent_view_document_cache(session_id);
    }

    /// Refreshes only the agent-view blocks owned by one changed transcript message when the current surface can stay structural.
    /// 当当前表面只需要结构化视图时，仅刷新单条转录消息拥有的 agent-view 区块。
    pub(super) fn refresh_session_agent_view_message_blocks(
        &mut self,
        session_id: SessionId,
        message_id: &str,
    ) -> bool {
        let mut perf_trace = TranscriptPerfTrace::begin(
            "refresh_session_agent_view_message_blocks",
            format!("session_id={session_id} message_id={message_id}"),
        );
        if self.should_materialize_session_agent_view_text_buffers(session_id) {
            perf_trace.finish();
            return false;
        }

        if self
            .floating_streaming_thinking_sessions
            .contains(&session_id)
            && self
                .streaming_thinking_message_ids
            .get(&session_id)
            .is_some_and(|active_message_id| active_message_id == message_id)
        {
            perf_trace.finish();
            return true;
        }

        let mut phase_started_at = Instant::now();
        let Some(message) = self.session_message_entry(session_id, message_id).cloned() else {
            perf_trace.checkpoint("load_message_entry", phase_started_at);
            perf_trace.finish();
            return false;
        };
        perf_trace.checkpoint("load_message_entry", phase_started_at);
        phase_started_at = Instant::now();
        let build_options = agent_view_build_options_for_streaming_output(
            self.streaming_output_message_ids
                .get(&session_id)
                .filter(|active_message_id| active_message_id.as_str() == message_id)
                .map(String::as_str),
            self.current_locale(),
        );
        let replacement_document = build_agent_view_document_with_options(
            std::slice::from_ref(&message),
            &[],
            &build_options,
        );
        perf_trace.checkpoint("build_replacement_document", phase_started_at);
        phase_started_at = Instant::now();
        let Some(document) = self.session_agent_view_documents.get_mut(&session_id) else {
            perf_trace.checkpoint("load_session_document", phase_started_at);
            perf_trace.finish();
            return false;
        };
        perf_trace.checkpoint("load_session_document", phase_started_at);

        phase_started_at = Instant::now();
        let matching_indexes = document
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                block
                    .attributes()
                    .get("message-id")
                    .is_some_and(|block_message_id| block_message_id == message_id)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let (Some(start_index), Some(end_index)) = (
            matching_indexes.first().copied(),
            matching_indexes.last().map(|index| index + 1),
        ) else {
            perf_trace.checkpoint("locate_message_block_range", phase_started_at);
            perf_trace.finish();
            return false;
        };
        if end_index - start_index != matching_indexes.len() {
            perf_trace.checkpoint("locate_message_block_range", phase_started_at);
            perf_trace.finish();
            return false;
        }
        perf_trace.checkpoint("locate_message_block_range", phase_started_at);

        let replacement_blocks = replacement_document.blocks;
        let replaced_block_count = end_index - start_index;
        phase_started_at = Instant::now();
        document
            .blocks
            .splice(start_index..end_index, replacement_blocks.clone());
        perf_trace.checkpoint("splice_session_blocks", phase_started_at);
        phase_started_at = Instant::now();
        self.refresh_session_inline_agent_view_message_blocks(
            session_id,
            start_index,
            replaced_block_count,
            &replacement_blocks,
        );
        perf_trace.checkpoint("refresh_inline_projection", phase_started_at);
        phase_started_at = Instant::now();
        self.refresh_active_modal_payloads_for_session(session_id);
        perf_trace.checkpoint("refresh_modal_payloads", phase_started_at);
        perf_trace.finish();
        true
    }

    /// Rebuilds the selectable structured/raw transcript projections from the already cached structural document.
    /// 基于已经缓存好的结构化文档重建可选结构化/原文转录投影。
    pub(super) fn refresh_session_agent_view_text_buffers(&mut self, session_id: SessionId) {
        let Some(document) = self.session_agent_view_documents.get(&session_id).cloned() else {
            self.clear_session_agent_view_text_buffers(session_id);
            return;
        };

        let selectable_document =
            vulcan_agent_view::AgentViewSelectableDocument::from_document(&document);
        let selectable_text = selectable_document.buffer().to_string();
        let raw_source_text = agent_view_document_raw_source(&document);

        self.session_agent_view_buffers.insert(
            session_id,
            text_editor::Content::with_text(&selectable_text),
        );
        self.session_agent_view_raw_buffers.insert(
            session_id,
            text_editor::Content::with_text(&raw_source_text),
        );
        self.session_agent_view_selectable_documents
            .insert(session_id, selectable_document);
    }

    /// Refreshes the structural transcript cache first, then materializes heavyweight text projections only when the active surface needs them.
    /// 先刷新结构化转录缓存，再仅在当前活动表面确实需要时物化较重的文本投影。
    pub(super) fn refresh_session_agent_view_buffers(&mut self, session_id: SessionId) {
        self.loading_session_agent_view_projection_ids
            .remove(&session_id);
        self.refresh_session_agent_view_document_cache(session_id);
        self.refresh_active_modal_payloads_for_session(session_id);

        if self.should_materialize_session_agent_view_text_buffers(session_id) {
            self.refresh_session_agent_view_text_buffers(session_id);
        } else {
            self.clear_session_agent_view_text_buffers(session_id);
        }
    }

    /// Refreshes active centered-modal payload snapshots for the provided session so animation ticks can reuse them without rescanning the whole transcript.
    /// 刷新给定会话当前激活的居中弹层快照载荷，使动画帧能够复用它们而无需重扫整份转录。
    pub(super) fn refresh_active_modal_payloads_for_session(&mut self, session_id: SessionId) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            let Some(block_id) = self
                .active_thinking_modal
                .as_ref()
                .map(|modal_state| modal_state.block_id.clone())
            else {
                return;
            };

            match self.build_thinking_modal_surface_payload(session_id, &block_id) {
                Some(payload) => {
                    if let Some(modal_state) = self.active_thinking_modal.as_mut() {
                        modal_state.payload = payload;
                    }
                }
                None => {
                    self.active_thinking_modal = None;
                }
            }
        }

        if self
            .active_tool_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            let Some(block_id) = self
                .active_tool_modal
                .as_ref()
                .map(|modal_state| modal_state.block_id.clone())
            else {
                return;
            };

            match self.build_tool_modal_surface_payload(session_id, &block_id) {
                Some(payload) => {
                    if let Some(modal_state) = self.active_tool_modal.as_mut() {
                        modal_state.payload = payload;
                    }
                }
                None => {
                    self.active_tool_modal = None;
                }
            }
        }
    }

    /// Synchronizes the unified block collapsed-state map with the latest document shape while preserving user toggles.
    /// 在保留用户切换结果的同时，将统一块折叠状态映射同步到最新文档结构。
    pub(super) fn sync_session_agent_view_block_collapsed_state(
        &mut self,
        session_id: SessionId,
        document: &vulcan_agent_view::AgentViewDocument,
    ) {
        self.session_agent_view_follow_tail
            .entry(session_id)
            .or_insert(true);
        let collapsed_entry = self
            .session_agent_view_block_collapsed
            .entry(session_id)
            .or_default();
        let mut active_ids = BTreeSet::new();

        for (block_index, block) in document.blocks.iter().enumerate() {
            let block_id = agent_view_block_id(session_id, block_index, block);
            collapsed_entry
                .entry(block_id.clone())
                .or_insert(agent_view_block_collapsed_by_default(block));
            active_ids.insert(block_id);
        }

        collapsed_entry.retain(|block_id, _| active_ids.contains(block_id));
    }

    /// Returns the visible session messages that should participate in the unified agent-view document for one session.
    /// 返回某个会话中应参与统一 agent-view 文档构建的可见消息集合。
    pub(super) fn visible_session_messages_for_agent_view(
        &self,
        session_id: SessionId,
    ) -> Vec<SessionMessageEntry> {
        let floating_thinking_message_id = if self.session_has_active_run(session_id)
            && self
                .floating_streaming_thinking_sessions
                .contains(&session_id)
        {
            self.streaming_thinking_message_ids
                .get(&session_id)
                .cloned()
        } else {
            None
        };

        self.session_messages
            .get(&session_id)
            .map(|messages| {
                merge_tool_display_messages(
                    messages
                        .iter()
                        .filter(|message| {
                            message.is_visible_to_user()
                                && floating_thinking_message_id
                                    .as_deref()
                                    .is_none_or(|active_id| message.id != active_id)
                        })
                        .cloned()
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    /// Projects one cached child conversation snapshot into the visible merged transcript rows used by the dedicated subagent panel.
    /// 将一份缓存的子会话快照投影为独立子代理面板使用的可见合并转录行集合。
    pub(super) fn subagent_task_visible_messages(
        &self,
        state: &SubagentTaskPanelState,
    ) -> Vec<SessionMessageEntry> {
        let Some(conversation_state) = state.conversation_state.as_ref() else {
            return Vec::new();
        };
        merge_tool_display_messages(
            conversation_state_to_session_messages(conversation_state)
                .into_iter()
                .filter(SessionMessageEntry::is_visible_to_user)
                .collect(),
        )
    }

    /// Builds the rendered transcript document shown inside the dedicated subagent panel when a child snapshot is available.
    /// 当子会话快照可用时，构建展示在独立子代理面板中的渲染转录文档。
    pub(super) fn rendered_subagent_task_panel_document(
        &self,
        state: &SubagentTaskPanelState,
    ) -> Option<vulcan_agent_view::AgentViewDocument> {
        if let Some(document) = self.rendered_session_agent_view_document(state.task_id) {
            return Some(document);
        }

        let messages = self.subagent_task_visible_messages(state);
        if messages.is_empty() {
            return None;
        }

        let build_options =
            agent_view_build_options_for_streaming_output(None, self.current_locale());
        let mut document = build_agent_view_document_with_options(&messages, &[], &build_options);
        for (block_index, block) in document.blocks.iter_mut().enumerate() {
            let block_id = agent_view_block_id(state.task_id, block_index, block);
            self.decorate_subagent_task_block_for_render(state.task_id, &block_id, block);
        }
        Some(document)
    }

    /// Returns whether the provided block belongs to the currently active workspace-history search highlight target.
    /// 返回给定区块是否属于当前活动中的工作台历史搜索高亮目标。
    pub(super) fn agent_view_block_matches_workspace_history_search_highlight(
        &self,
        session_id: SessionId,
        block: &vulcan_agent_view::AgentViewBlock,
    ) -> bool {
        let Some(active_highlight) = self
            .workspace_history_search
            .active_highlight
            .as_ref()
            .filter(|active_highlight| active_highlight.session_id == session_id)
        else {
            return false;
        };

        block
            .attributes()
            .get("message-id")
            .is_some_and(|message_id| message_id == &active_highlight.message_id)
    }

    /// Attaches the transient workspace-history search-hit render marker to one block when it owns the currently highlighted message.
    /// 当某个区块归属于当前高亮消息时，为其附着临时工作台历史搜索命中渲染标记。
    pub(super) fn decorate_agent_view_block_workspace_history_search_hit(
        &self,
        session_id: SessionId,
        block: &mut vulcan_agent_view::AgentViewBlock,
    ) {
        if self.agent_view_block_matches_workspace_history_search_highlight(session_id, block) {
            block
                .attributes_mut()
                .insert(String::from("ui-search-hit"), String::from("true"));
        }
    }

    /// Decorates one child-panel block with the minimal runtime-only attributes needed by the shared transcript renderer.
    /// 使用共享转录渲染器所需的最小运行时属性装饰单个子面板区块。
    pub(super) fn decorate_subagent_task_block_for_render(
        &self,
        session_id: SessionId,
        block_id: &str,
        block: &mut vulcan_agent_view::AgentViewBlock,
    ) {
        self.decorate_agent_view_block_workspace_history_search_hit(session_id, block);

        let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
            return;
        };

        directive
            .attributes
            .insert(String::from("ui-block-id"), block_id.to_string());
        if directive.kind == vulcan_agent_view::AgentViewDirectiveKind::Thinking {
            directive
                .attributes
                .insert(String::from("ui-inline-collapsed"), String::from("false"));
            directive.attributes.insert(
                String::from("ui-render-thinking-header"),
                String::from("true"),
            );
            directive
                .attributes
                .insert(String::from("ui-thinking-surface"), String::from("inline"));
            directive.attributes.insert(
                String::from("ui-thinking-title"),
                build_thinking_title(
                    &directive.body,
                    &self.t(I18nKey::TranscriptSubagentThinking),
                ),
            );
        }
    }

    /// Returns the currently streaming thinking message for one session when it is still actively floating above the transcript.
    /// 返回某个会话当前仍以浮层方式展示的流式思考消息。
    pub(super) fn streaming_thinking_message(
        &self,
        session_id: SessionId,
    ) -> Option<&SessionMessageEntry> {
        if !self.session_has_active_run(session_id) {
            return None;
        }
        if !self
            .floating_streaming_thinking_sessions
            .contains(&session_id)
        {
            return None;
        }

        let message_id = self.streaming_thinking_message_ids.get(&session_id)?;
        self.session_messages
            .get(&session_id)?
            .iter()
            .find(|message| message.id == *message_id && !message.body.trim().is_empty())
    }

    /// Returns the cached runtime-decorated transcript document used by the primary rendered transcript surface.
    /// 返回主渲染转录表面使用的缓存运行时增强文档。
    pub(super) fn rendered_session_agent_view_document(
        &self,
        session_id: SessionId,
    ) -> Option<vulcan_agent_view::AgentViewDocument> {
        let mut document = self
            .session_agent_view_documents
            .get(&session_id)
            .cloned()?;

        for (block_index, block) in document.blocks.iter_mut().enumerate() {
            let block_id = agent_view_block_id(session_id, block_index, block);
            self.decorate_agent_view_block_for_render(session_id, &block_id, block);
        }

        Some(document)
    }

    /// Decorates one agent-view block with runtime-only rendering metadata such as collapse state and thinking metrics.
    /// 使用折叠状态与思考指标等仅运行时元数据装饰单个 agent-view 区块。
    pub(super) fn decorate_agent_view_block_for_render(
        &self,
        session_id: SessionId,
        block_id: &str,
        block: &mut vulcan_agent_view::AgentViewBlock,
    ) {
        self.decorate_agent_view_block_workspace_history_search_hit(session_id, block);

        let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
            return;
        };

        directive
            .attributes
            .insert(String::from("ui-block-id"), block_id.to_string());

        if let Some(local_path) = directive
            .attributes
            .get("generated-media-local-path")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            directive.attributes.insert(
                String::from("ui-generated-media-playing"),
                self.is_generated_audio_playing_local_path(local_path)
                    .to_string(),
            );
        }

        if directive.kind == vulcan_agent_view::AgentViewDirectiveKind::Thinking {
            directive
                .attributes
                .insert(String::from("ui-inline-collapsed"), String::from("false"));
            directive.attributes.insert(
                String::from("ui-render-thinking-header"),
                String::from("true"),
            );
            directive
                .attributes
                .insert(String::from("ui-thinking-surface"), String::from("inline"));
            directive.attributes.insert(
                String::from("ui-thinking-title"),
                self.session_thinking_title(session_id, directive),
            );
        }
    }

    /// Builds the inline thinking title shown in both the transcript header row and the explicit centered modal.
    /// 构建展示在转录标题行与显式居中弹层中的内联思考标题。
    pub(super) fn session_thinking_title(
        &self,
        session_id: SessionId,
        directive: &vulcan_agent_view::AgentViewDirectiveBlock,
    ) -> String {
        let duration_label = directive
            .attributes
            .get("message-id")
            .and_then(|message_id| self.session_thinking_duration_millis(session_id, message_id))
            .map(format_thinking_duration_label)
            .unwrap_or_else(|| self.t(I18nKey::CommonUnknownDuration));
        build_thinking_title(&directive.body, &duration_label)
    }

    /// Returns the best-effort thinking duration derived from the thinking message timestamp and the next visible message or runtime snapshot.
    /// 根据思考消息时间戳与下一条可见消息或运行时快照推导最佳努力思考时长。
    pub(super) fn session_thinking_duration_millis(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<u128> {
        let messages = self.session_messages.get(&session_id)?;
        let message_index = messages
            .iter()
            .position(|message| message.id == message_id)?;
        let started_at = parse_unix_millis_text(&messages.get(message_index)?.created_at)?;

        let finished_at = messages
            .iter()
            .skip(message_index + 1)
            .find(|message| {
                message.is_visible_to_user()
                    && message.record_kind != SessionMessageRecordKind::ThinkingOutput
                    && message.kind != SessionMessageKind::User
                    && !message.created_at.trim().is_empty()
            })
            .and_then(|message| parse_unix_millis_text(&message.created_at))
            .or_else(|| {
                if self.session_has_active_run(session_id)
                    && self
                        .streaming_thinking_message_ids
                        .get(&session_id)
                        .is_some_and(|active_id| active_id == message_id)
                    && !self.streaming_output_message_ids.contains_key(&session_id)
                {
                    Some(current_unix_millis())
                } else {
                    self.session_runtime_state
                        .get(&session_id)
                        .and_then(|runtime_state| parse_unix_millis_text(&runtime_state.updated_at))
                }
            })?;

        Some(finished_at.saturating_sub(started_at))
    }

    /// Returns the current loading progress snapshot for one session transcript surface when hydration or projection materialization is still in flight.
    /// 当水合或投影物化仍在进行中时，返回单个会话转录表面的当前加载进度快照。
    pub(super) fn session_agent_view_loading_progress(
        &self,
        session_id: SessionId,
    ) -> Option<(f32, String)> {
        // Keep already-renderable cached transcripts visible while a background refresh is in flight.
        // 后台刷新仍在进行时保留已经可渲染的缓存转录，避免切换时出现不必要的整页等待。
        let has_cached_document = self
            .session_agent_view_documents
            .get(&session_id)
            .is_some_and(|document| !document.blocks.is_empty());
        if self.loading_session_ids.contains(&session_id) && !has_cached_document {
            return Some((0.38, self.t(I18nKey::StatusSessionLoading)));
        }

        if self
            .loading_session_agent_view_projection_ids
            .contains(&session_id)
            && !has_cached_document
        {
            return Some((
                0.76,
                self.t(I18nKey::StatusProjectionBuilding),
            ));
        }

        None
    }

    /// Builds one stable centered-thinking modal payload snapshot from the latest rendered session document.
    /// 基于最新渲染的会话文档构建一份稳定的居中思考弹层载荷快照。
    pub(super) fn build_thinking_modal_surface_payload(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<ModalSurfacePayload> {
        let document = self.rendered_session_agent_view_document(session_id)?;
        let (title, document) = build_modal_thinking_surface_document(document, block_id)?;

        Some(self.build_modal_surface_payload(
            title,
            self.t(I18nKey::TranscriptThinkingModalHint),
            document,
            Vec::new(),
        ))
    }

    /// Builds one stable centered-tool modal payload snapshot from the latest rendered session document.
    /// 基于最新渲染的会话文档构建一份稳定的居中工具弹层载荷快照。
    pub(super) fn build_tool_modal_surface_payload(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<ModalSurfacePayload> {
        let resolved_message = self.resolve_rendered_tool_modal_message_entry(session_id, block_id);
        let generated_attachments = resolved_message
            .as_ref()
            .and_then(|message| message.metadata.as_ref())
            .map(generated_attachments_from_tool_metadata)
            .unwrap_or_default();
        if let Some(message) = resolved_message.as_ref()
            && tool_display_message_is_task_session(message)
            && let Some(metadata) = message.metadata.as_ref()
            && let Some(task_id) = task_session_payload_from_tool_metadata(metadata)
                .and_then(task_session_internal_id_from_payload)
            && !self.subagent_task_panel_has_live_detail(task_id)
            && let Some(markdown_body) = task_session_modal_markdown_from_tool_metadata(metadata)
        {
            let title = if message.title.trim().is_empty() {
                self.t(I18nKey::TranscriptSubagentResultSummaryTitle)
            } else {
                message.title.clone()
            };
            return Some(self.build_modal_surface_payload(
                title,
                self.t(I18nKey::TranscriptSubagentResultSummaryHint),
                parse_agent_view_document(&markdown_body),
                generated_attachments,
            ));
        }

        if let Some(message) = resolved_message {
            let title = if message.title.trim().is_empty() {
                self.t(I18nKey::TranscriptToolModalTitle)
            } else {
                message.title.clone()
            };
            return Some(self.build_modal_surface_payload(
                title,
                self.t(I18nKey::TranscriptToolModalHint),
                parse_agent_view_document(&message.body),
                generated_attachments,
            ));
        }

        let document = self.rendered_session_agent_view_document(session_id)?;
        let (title, document) = build_modal_tool_surface_document(document, block_id)?;

        Some(self.build_modal_surface_payload(
            title,
            self.t(I18nKey::TranscriptToolModalHint),
            document,
            generated_attachments,
        ))
    }

    /// Packs one modal title, helper copy, and document snapshot into a reusable payload with a stable lazy-render signature.
    /// 将弹层标题、辅助文案与文档快照打包成可复用载荷，并附带稳定的惰性渲染签名。
    pub(super) fn build_modal_surface_payload(
        &self,
        title: String,
        helper_text: String,
        document: vulcan_agent_view::AgentViewDocument,
        generated_attachments: Vec<ConversationAttachment>,
    ) -> ModalSurfacePayload {
        ModalSurfacePayload {
            title,
            helper_text,
            generated_attachments,
            document_signature: self.hash_agent_view_document(&document),
            document,
        }
    }

    /// Hashes one agent-view document into a stable signature used to invalidate lazy modal-body rebuilds only when the rendered content really changes.
    /// 将一份 agent-view 文档哈希为稳定签名，仅在实际渲染内容变化时触发惰性弹层正文重建失效。
    pub(super) fn hash_agent_view_document(
        &self,
        document: &vulcan_agent_view::AgentViewDocument,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();

        for block in &document.blocks {
            block.raw_source().hash(&mut hasher);
            for (key, value) in block.attributes() {
                key.hash(&mut hasher);
                value.hash(&mut hasher);
            }
        }

        document.blocks.len().hash(&mut hasher);
        hasher.finish()
    }
}

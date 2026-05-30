//! Live session progress, transcript scrolling, and modal runtime helpers for the main workbench.
//! 主工作台的实时会话进度、转录滚动与弹层运行态辅助逻辑。

use super::*;

impl VulcanWorkbench {
    /// Updates the execution phase tracked for one session transcript.
    /// 更新某个会话转录区跟踪的执行阶段。
    pub(super) fn set_session_run_phase(&mut self, session_id: SessionId, phase: SessionRunPhase) {
        self.session_run_phases.insert(session_id, phase);
    }

    /// Stores the current hook-declared precheck progress labels for one session.
    /// 保存某个会话当前由 hook 声明的 precheck 进度标签。
    pub(super) fn set_session_precheck_progress(
        &mut self,
        session_id: SessionId,
        labels: impl IntoIterator<Item = String>,
    ) {
        if let Some(progress) = SessionPrecheckProgressState::new(labels) {
            self.session_precheck_progress.insert(session_id, progress);
        } else {
            self.session_precheck_progress.remove(&session_id);
        }
    }

    /// Clears hook-declared precheck progress labels for one session.
    /// 清除某个会话由 hook 声明的 precheck 进度标签。
    pub(super) fn clear_session_precheck_progress(&mut self, session_id: SessionId) {
        self.session_precheck_progress.remove(&session_id);
    }

    /// Returns whether one specific session currently owns an active real-AI run.
    /// 返回某个指定会话当前是否拥有活动的真实 AI 运行。
    pub(super) fn session_has_active_run(&self, session_id: SessionId) -> bool {
        self.active_runs.contains_key(&session_id)
    }

    /// Returns whether one specific session currently owns an active prompt-compaction task.
    /// 返回某个指定会话当前是否拥有活动的提示词压缩任务。
    pub(super) fn session_has_active_compaction(&self, session_id: SessionId) -> bool {
        self.active_session_compactions.contains_key(&session_id)
    }

    /// Returns whether one specific session currently owns any active provider-backed transcript stream.
    /// 返回某个指定会话当前是否拥有任一活动中的 Provider 转录流。
    pub(super) fn session_has_active_transcript_stream(&self, session_id: SessionId) -> bool {
        self.session_has_active_run(session_id) || self.session_has_active_compaction(session_id)
    }

    /// Returns whether any lightweight runtime-progress animation still needs ticking.
    /// 返回当前是否仍有任一轻量运行进度动画需要继续 Tick。
    pub(super) fn has_runtime_progress_animation_work(&self) -> bool {
        !self.pending_provider_response_sessions.is_empty()
            || !self.floating_streaming_thinking_sessions.is_empty()
            || !self.session_precheck_progress.is_empty()
    }

    /// Marks one session as waiting for the next provider-visible response chunk after a request has already been sent.
    /// 将某个会话标记为“请求已发送，正在等待下一段 Provider 可见响应块”。
    pub(super) fn mark_session_pending_provider_response(&mut self, session_id: SessionId) {
        self.pending_provider_response_sessions.insert(session_id);
    }

    /// Clears the pending-provider-response marker once the session has started showing visible progress again.
    /// 一旦会话重新开始展示可见进度，清除其“等待 Provider 响应”标记。
    pub(super) fn clear_session_pending_provider_response(&mut self, session_id: SessionId) {
        self.pending_provider_response_sessions.remove(&session_id);
    }

    /// Returns the shared animated suffix used by lightweight runtime progress hints.
    /// 返回轻量运行进度提示共用的动画后缀。
    pub(super) fn runtime_progress_ellipsis(&self) -> &'static str {
        match self.runtime_progress_animation_frame % 3 {
            0 => ".",
            1 => "..",
            _ => "...",
        }
    }

    /// Returns the localized lightweight request-progress hint shown before the first visible provider delta arrives.
    /// 返回在首个可见 Provider 增量到来前展示的本地化轻量请求进度提示。
    pub(super) fn session_requesting_progress_text(&self, session_id: SessionId) -> Option<String> {
        if self.session_run_phase_for(session_id) == SessionRunPhase::Preparing
            && !self.session_has_active_run(session_id)
            && !self.session_has_active_compaction(session_id)
            && let Some(progress) = self.session_precheck_progress.get(&session_id)
        {
            let suffix = self.runtime_progress_ellipsis();
            return Some(format!("{}{suffix}", progress.primary_label()));
        }
        if !self
            .pending_provider_response_sessions
            .contains(&session_id)
        {
            return None;
        }
        let suffix = self.runtime_progress_ellipsis();
        Some(match self.current_locale() {
            AppLocale::ZhCn => format!("正在请求{suffix}"),
            AppLocale::EnUs => format!("Requesting{suffix}"),
        })
    }

    /// Returns the localized lightweight inline-thinking placeholder title while the visible reasoning block is still streaming.
    /// 当可见思考区块仍在流式进行中时，返回其本地化轻量占位标题。
    pub(super) fn session_streaming_thinking_progress_title(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<String> {
        let active_message_id = self.streaming_thinking_message_ids.get(&session_id)?;
        if active_message_id != message_id
            || !self
                .floating_streaming_thinking_sessions
                .contains(&session_id)
        {
            return None;
        }
        let suffix = self.runtime_progress_ellipsis();
        Some(match self.current_locale() {
            AppLocale::ZhCn => format!("正在思考{suffix}"),
            AppLocale::EnUs => format!("Thinking{suffix}"),
        })
    }

    /// Clears the active run state tracked for one session after its stream finishes or is stopped.
    /// 在某个会话的流结束或被停止后清理其活动运行状态。
    pub(super) fn clear_active_run_tracking(&mut self, session_id: SessionId) {
        self.active_runs.remove(&session_id);
        self.pending_provider_response_sessions.remove(&session_id);
        self.session_precheck_progress.remove(&session_id);
    }

    /// Removes and returns the active run state tracked for one session.
    /// 移除并返回某个会话当前跟踪的活动运行状态。
    pub(super) fn take_active_run_state(
        &mut self,
        session_id: SessionId,
    ) -> Option<ActiveSessionRunState> {
        self.active_runs.remove(&session_id)
    }

    /// Removes and returns the active prompt-compaction state tracked for one session.
    /// 移除并返回某个会话当前跟踪的活动提示词压缩状态。
    pub(super) fn take_active_compaction_state(
        &mut self,
        session_id: SessionId,
    ) -> Option<ActiveSessionCompactionState> {
        self.active_session_compactions.remove(&session_id)
    }

    /// Returns the session identifier that owns the provided run identifier, if it is still active.
    /// 返回持有指定运行标识的会话标识（如果该运行仍然处于活动状态）。
    pub(super) fn active_run_session_id_for_run_id(&self, run_id: u64) -> Option<SessionId> {
        self.active_runs
            .iter()
            .find_map(|(session_id, active_run)| {
                if active_run.run_id == run_id {
                    Some(*session_id)
                } else {
                    None
                }
            })
    }

    /// Returns the session identifier that owns the provided prompt-compaction task identifier, if it is still active.
    /// 返回持有指定提示词压缩任务标识的会话标识（如果该任务仍然处于活动状态）。
    pub(super) fn active_compaction_session_id_for_run_id(&self, run_id: u64) -> Option<SessionId> {
        self.active_session_compactions
            .iter()
            .find_map(|(session_id, active_compaction)| {
                if active_compaction.run_id == run_id {
                    Some(*session_id)
                } else {
                    None
                }
            })
    }

    /// Returns whether the provided generated-media local path currently matches the active inline audio playback entry.
    /// 返回给定生成媒体本地路径当前是否匹配活动中的内联音频播放条目。
    pub(super) fn is_generated_audio_playing_local_path(&self, local_path: &str) -> bool {
        self.generated_audio_playback
            .as_ref()
            .is_some_and(|playback| playback.local_path == local_path && !playback.sink.empty())
    }

    /// Returns whether one session transcript should keep auto-following the latest tail output.
    /// 返回某个会话转录是否应继续自动追随最新尾部输出。
    pub(super) fn session_agent_view_should_follow_tail(&self, session_id: SessionId) -> bool {
        self.session_agent_view_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped auto-follow flag from one reported transcript viewport.
    /// 根据一次回传的转录视口更新会话级自动追随标记。
    pub(super) fn update_session_agent_view_follow_tail(
        &mut self,
        session_id: SessionId,
        viewport: scrollable::Viewport,
    ) {
        self.session_agent_view_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session transcript to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话转录吸附到尾部的任务。
    pub(super) fn maybe_follow_selected_session_transcript_tail(
        &self,
        session_id: SessionId,
    ) -> Task<Message> {
        if self.active_transcript_surface_session_id() != Some(session_id) {
            return Task::none();
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(transcript_scrollable_id(session_id))
    }

    /// Records one streaming transcript tail-follow request so repeated token updates share one timed scroll operation.
    /// 记录一次流式转录尾部追随请求，使重复 token 更新共享一次定时滚动操作。
    pub(super) fn request_selected_session_transcript_tail_follow(
        &mut self,
        session_id: SessionId,
    ) {
        if self.active_transcript_surface_session_id() != Some(session_id) {
            return;
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return;
        }

        self.pending_transcript_follow_tail_session_ids
            .insert(session_id);
    }

    /// Applies one coalesced transcript tail-follow request for the currently visible transcript surface.
    /// 为当前可见转录表面执行一次已合并的尾部追随请求。
    pub(super) fn perform_pending_transcript_tail_follow_tick(&mut self) -> Task<Message> {
        let pending_session_ids =
            std::mem::take(&mut self.pending_transcript_follow_tail_session_ids);
        let Some(session_id) = self.active_transcript_surface_session_id() else {
            return Task::none();
        };

        if !pending_session_ids.contains(&session_id) {
            return Task::none();
        }

        if !self.session_agent_view_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(transcript_scrollable_id(session_id))
    }

    /// Restores the last reported transcript scroll offset when the scrollable surface is remounted without a session switch.
    /// 当转录滚动表面在未切换会话的情况下重新挂载时，恢复最后一次回传的滚动偏移。
    pub(super) fn restore_selected_session_transcript_scroll_position(
        &self,
        session_id: SessionId,
    ) -> Task<Message> {
        if self.active_transcript_surface_session_id() != Some(session_id) {
            return Task::none();
        }

        if self.session_agent_view_should_follow_tail(session_id) {
            return self.maybe_follow_selected_session_transcript_tail(session_id);
        }

        let Some(offset) = self
            .session_agent_view_viewports
            .get(&session_id)
            .map(scrollable::Viewport::absolute_offset)
        else {
            return Task::none();
        };

        operation::scroll_to(transcript_scrollable_id(session_id), offset)
    }

    /// Updates the active drag auto-scroll state for the main rendered transcript surface.
    /// 更新主渲染转录表面的当前拖拽自动滚动状态。
    pub(super) fn update_transcript_drag_auto_scroll_state(
        &mut self,
        session_id: SessionId,
        request: Option<SelectableTranscriptDragAutoScroll>,
    ) {
        if self.active_transcript_surface_session_id() != Some(session_id) {
            self.clear_transcript_drag_auto_scroll_for_session(session_id);
            return;
        }

        self.active_transcript_drag_auto_scroll =
            request.map(|request| ActiveTranscriptDragAutoScroll {
                session_id,
                request,
            });
    }

    /// Updates the active drag auto-scroll state for the floating thinking overlay body.
    /// 更新流式思考浮层正文的当前拖拽自动滚动状态。
    pub(super) fn update_thinking_overlay_drag_auto_scroll_state(
        &mut self,
        session_id: SessionId,
        request: Option<SelectableTranscriptDragAutoScroll>,
    ) {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            self.clear_thinking_overlay_drag_auto_scroll(session_id);
            return;
        }

        self.active_thinking_overlay_drag_auto_scroll =
            request.map(|request| ActiveTranscriptDragAutoScroll {
                session_id,
                request,
            });
    }

    /// Clears both transcript drag auto-scroll snapshots regardless of which surface currently owns them.
    /// 无论当前由哪个表面持有，统一清空两类转录拖拽自动滚动快照。
    pub(super) fn clear_transcript_drag_auto_scrolls(&mut self) {
        self.active_transcript_drag_auto_scroll = None;
        self.active_thinking_overlay_drag_auto_scroll = None;
    }

    /// Clears the main transcript drag auto-scroll snapshot when it belongs to the provided session.
    /// 当主转录拖拽自动滚动快照属于给定会话时清空它。
    pub(super) fn clear_transcript_drag_auto_scroll_for_session(&mut self, session_id: SessionId) {
        if self
            .active_transcript_drag_auto_scroll
            .as_ref()
            .is_some_and(|state| state.session_id == session_id)
        {
            self.active_transcript_drag_auto_scroll = None;
        }
    }

    /// Clears the floating thinking overlay drag auto-scroll snapshot when it belongs to the provided session.
    /// 当流式思考浮层拖拽自动滚动快照属于给定会话时清空它。
    pub(super) fn clear_thinking_overlay_drag_auto_scroll(&mut self, session_id: SessionId) {
        if self
            .active_thinking_overlay_drag_auto_scroll
            .as_ref()
            .is_some_and(|state| state.session_id == session_id)
        {
            self.active_thinking_overlay_drag_auto_scroll = None;
        }
    }

    /// Performs one timed drag auto-scroll tick for the main rendered transcript surface.
    /// 为主渲染转录表面执行一次定时拖拽自动滚动 Tick。
    pub(super) fn perform_transcript_drag_auto_scroll_tick(&mut self) -> Task<Message> {
        let Some(active_state) = self.active_transcript_drag_auto_scroll else {
            return Task::none();
        };

        if self.active_transcript_surface_session_id() != Some(active_state.session_id) {
            self.clear_transcript_drag_auto_scroll_for_session(active_state.session_id);
            return Task::none();
        }

        let delta_y = drag_auto_scroll_step(active_state.request);

        if delta_y.abs() <= f32::EPSILON {
            self.clear_transcript_drag_auto_scroll_for_session(active_state.session_id);
            return Task::none();
        }

        operation::scroll_by(
            transcript_scrollable_id(active_state.session_id),
            scrollable::AbsoluteOffset { x: 0.0, y: delta_y },
        )
    }

    /// Performs one timed drag auto-scroll tick for the floating thinking overlay body.
    /// 为流式思考浮层正文执行一次定时拖拽自动滚动 Tick。
    pub(super) fn perform_thinking_overlay_drag_auto_scroll_tick(&mut self) -> Task<Message> {
        let Some(active_state) = self.active_thinking_overlay_drag_auto_scroll else {
            return Task::none();
        };

        if self.selected_session().map(|session| session.id) != Some(active_state.session_id) {
            self.clear_thinking_overlay_drag_auto_scroll(active_state.session_id);
            return Task::none();
        }

        let delta_y = drag_auto_scroll_step(active_state.request);

        if delta_y.abs() <= f32::EPSILON {
            self.clear_thinking_overlay_drag_auto_scroll(active_state.session_id);
            return Task::none();
        }

        operation::scroll_by(
            thinking_overlay_scrollable_id(active_state.session_id),
            scrollable::AbsoluteOffset { x: 0.0, y: delta_y },
        )
    }

    /// Returns whether one floating streaming-thinking overlay should keep auto-following the latest tail.
    /// 返回某个流式思考浮层是否应继续自动追随最新尾部。
    pub(super) fn session_streaming_thinking_should_follow_tail(
        &self,
        session_id: SessionId,
    ) -> bool {
        self.session_streaming_thinking_follow_tail
            .get(&session_id)
            .copied()
            .unwrap_or(true)
    }

    /// Updates the session-scoped floating-thinking auto-follow flag from one reported overlay viewport.
    /// 根据一次回传的浮层视口更新会话级流式思考自动追随标记。
    pub(super) fn update_session_streaming_thinking_follow_tail(
        &mut self,
        session_id: SessionId,
        viewport: scrollable::Viewport,
    ) {
        self.session_streaming_thinking_follow_tail
            .insert(session_id, transcript_viewport_is_near_bottom(viewport));
    }

    /// Builds one task that snaps the selected-session floating-thinking overlay to its tail when the user has not manually scrolled away.
    /// 当用户尚未手动滚离底部时，构建将当前选中会话流式思考浮层吸附到尾部的任务。
    pub(super) fn maybe_follow_selected_session_thinking_overlay_tail(
        &self,
        session_id: SessionId,
    ) -> Task<Message> {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return Task::none();
        }

        if self.streaming_thinking_message(session_id).is_none() {
            return Task::none();
        }

        if !self.session_streaming_thinking_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(thinking_overlay_scrollable_id(session_id))
    }

    /// Records one streaming thinking-overlay tail-follow request so repeated token updates share one timed scroll operation.
    /// 记录一次流式思考浮层尾部追随请求，使重复 token 更新共享一次定时滚动操作。
    pub(super) fn request_selected_session_thinking_overlay_tail_follow(
        &mut self,
        session_id: SessionId,
    ) {
        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return;
        }

        if self.streaming_thinking_message(session_id).is_none() {
            return;
        }

        if !self.session_streaming_thinking_should_follow_tail(session_id) {
            return;
        }

        self.pending_thinking_overlay_follow_tail_session_ids
            .insert(session_id);
    }

    /// Applies one coalesced floating thinking-overlay tail-follow request for the currently selected session.
    /// 为当前选中会话执行一次已合并的流式思考浮层尾部追随请求。
    pub(super) fn perform_pending_thinking_overlay_tail_follow_tick(&mut self) -> Task<Message> {
        let pending_session_ids =
            std::mem::take(&mut self.pending_thinking_overlay_follow_tail_session_ids);
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return Task::none();
        };

        if !pending_session_ids.contains(&session_id) {
            return Task::none();
        }

        if self.streaming_thinking_message(session_id).is_none() {
            return Task::none();
        }

        if !self.session_streaming_thinking_should_follow_tail(session_id) {
            return Task::none();
        }

        operation::snap_to_end(thinking_overlay_scrollable_id(session_id))
    }

    /// Opens one centered thinking modal for the provided block identifier and restarts the opening animation.
    /// 为给定区块标识打开一个居中思考弹层，并重新启动入场动画。
    pub(super) fn open_thinking_modal(&mut self, session_id: SessionId, block_id: String) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| {
                modal_state.session_id == session_id
                    && modal_state.block_id == block_id
                    && modal_state.phase != ThinkingModalPhase::Closing
            })
        {
            return;
        }

        let Some(payload) = self.build_thinking_modal_surface_payload(session_id, &block_id) else {
            return;
        };

        self.active_tool_modal = None;
        self.active_thinking_modal = Some(ThinkingModalState {
            session_id,
            block_id,
            phase: ThinkingModalPhase::Opening,
            started_at: Instant::now(),
            payload,
        });
    }

    /// Opens one centered tool modal for the provided block identifier and restarts the opening animation.
    /// 为给定区块标识打开一个居中工具弹层，并重新启动入场动画。
    pub(super) fn open_tool_modal(&mut self, session_id: SessionId, block_id: String) {
        if self.active_tool_modal.as_ref().is_some_and(|modal_state| {
            modal_state.session_id == session_id
                && modal_state.block_id == block_id
                && modal_state.phase != ThinkingModalPhase::Closing
        }) {
            return;
        }

        let Some(payload) = self.build_tool_modal_surface_payload(session_id, &block_id) else {
            return;
        };

        self.active_thinking_modal = None;
        self.active_tool_modal = Some(ToolModalState {
            session_id,
            block_id,
            phase: ThinkingModalPhase::Opening,
            started_at: Instant::now(),
            payload,
            command_session_stop_in_flight: false,
        });
    }

    /// Starts closing the centered thinking modal when it is currently visible.
    /// 当居中思考弹层当前可见时，启动它的关闭动画。
    pub(super) fn begin_close_thinking_modal(&mut self) {
        if let Some(modal_state) = self.active_thinking_modal.as_mut() {
            if modal_state.phase != ThinkingModalPhase::Closing {
                modal_state.phase = ThinkingModalPhase::Closing;
                modal_state.started_at = Instant::now();
            }
        }
    }

    /// Starts closing the centered tool modal when it is currently visible.
    /// 当居中工具弹层当前可见时，启动它的关闭动画。
    pub(super) fn begin_close_tool_modal(&mut self) {
        if let Some(modal_state) = self.active_tool_modal.as_mut() {
            if modal_state.phase != ThinkingModalPhase::Closing {
                modal_state.phase = ThinkingModalPhase::Closing;
                modal_state.started_at = Instant::now();
            }
        }
    }

    /// Clears the centered thinking modal immediately when it belongs to the provided session.
    /// 当居中思考弹层属于给定会话时，立即清理该弹层。
    pub(super) fn close_thinking_modal_for_session(&mut self, session_id: SessionId) {
        if self
            .active_thinking_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            self.active_thinking_modal = None;
        }
    }

    /// Clears the centered tool modal immediately when it belongs to the provided session.
    /// 当居中工具弹层属于给定会话时，立即清理该弹层。
    pub(super) fn close_tool_modal_for_session(&mut self, session_id: SessionId) {
        if self
            .active_tool_modal
            .as_ref()
            .is_some_and(|modal_state| modal_state.session_id == session_id)
        {
            self.active_tool_modal = None;
        }
    }

    /// Returns one best-effort live runtime snapshot for the requested command-session lookup, preferring the historical owner id when available but falling back to the currently rendered session when needed.
    /// 为指定命令会话查询返回一份尽力而为的实时运行时快照，在可用时优先使用历史 owner id，并在必要时回退到当前渲染会话。
    pub(super) fn live_command_session_snapshot_for_lookup(
        &self,
        fallback_owner_session_id: SessionId,
        command_session_id: SessionId,
        owner_session_id: Option<SessionId>,
    ) -> Option<CommandSessionSnapshot> {
        let command_session_runtime = self.runtime_foundation.command_session_runtime();
        owner_session_id
            .and_then(|owner_session_id| {
                command_session_runtime
                    .try_snapshot_owned_session(owner_session_id, command_session_id)
            })
            .or_else(|| {
                command_session_runtime
                    .try_snapshot_owned_session(fallback_owner_session_id, command_session_id)
            })
            .or_else(|| command_session_runtime.try_snapshot_session_by_id(command_session_id))
    }

    /// Returns one command-session modal summary overlaid with the latest runtime snapshot when a live session still exists, while preserving the historical transcript payload as the fallback for archived sessions.
    /// 当实时会话仍存在时，返回一份叠加了最新运行时快照的命令会话弹层摘要；对于归档会话，则保留历史转录载荷作为回退。
    pub(super) fn command_session_summary_with_live_runtime_state(
        &self,
        fallback_owner_session_id: SessionId,
        summary: &CommandSessionToolModalSummary,
    ) -> CommandSessionToolModalSummary {
        let Some(snapshot) = self.live_command_session_snapshot_for_lookup(
            fallback_owner_session_id,
            summary.command_session_id,
            summary.owner_session_id,
        ) else {
            let mut summary = summary.clone();
            summary.mark_runtime_missing_as_stopped();
            return summary;
        };
        let mut summary = summary.clone();
        let _ = summary.apply_runtime_snapshot(&snapshot);
        summary
    }

    /// Returns one tool-modal presentation clone whose command-session summary fields are refreshed from the live runtime whenever the referenced session still exists.
    /// 返回一份工具弹层展示副本；只要被引用的命令会话仍存在，就会用实时运行时刷新其中的命令会话摘要字段。
    pub(super) fn tool_modal_presentation_with_live_command_session_state(
        &self,
        fallback_owner_session_id: SessionId,
        presentation: &ToolModalPresentation,
    ) -> ToolModalPresentation {
        match presentation {
            ToolModalPresentation::CommandSession(summary) => {
                ToolModalPresentation::CommandSession(
                    self.command_session_summary_with_live_runtime_state(
                        fallback_owner_session_id,
                        summary,
                    ),
                )
            }
            _ => presentation.clone(),
        }
    }

    /// Returns the owner/session identifiers of the currently opened running command-session modal when one exists.
    /// 当存在时，返回当前已打开且仍在运行的命令会话弹层对应的归属会话标识与命令会话标识。
    pub(super) fn active_command_session_modal_stop_target(
        &self,
    ) -> Option<(SessionId, SessionId)> {
        let modal_state = self.active_tool_modal.as_ref()?;
        let live_presentation = self.tool_modal_presentation_with_live_command_session_state(
            modal_state.session_id,
            &modal_state.payload.tool_modal_presentation,
        );
        let summary = live_presentation.command_session_summary()?;
        (summary.status == "running").then_some((
            summary.owner_session_id.unwrap_or(modal_state.session_id),
            summary.command_session_id,
        ))
    }

    /// Returns the stable tool-call identifier associated with the currently opened command-session modal when one can still be recovered from the rendered transcript block.
    /// 当仍能从已渲染转录区块恢复时，返回当前已打开命令会话弹层关联的稳定工具调用标识。
    pub(super) fn active_command_session_modal_tool_call_id(&self) -> Option<String> {
        let modal_state = self.active_tool_modal.as_ref()?;
        self.resolve_rendered_tool_modal_message_entry(
            modal_state.session_id,
            &modal_state.block_id,
        )
        .and_then(|message| tool_display_message_call_id(&message))
    }

    /// Appends one GUI-originated command-session stop result back into the canonical transcript so later reloads and modal rebuilds can observe the same terminal fact.
    /// 将一条源自 GUI 的命令会话停止结果追加回规范转录，使后续重载与弹层重建都能观测到同一终态事实。
    pub(super) fn append_command_session_stop_result_message(
        &mut self,
        session_id: SessionId,
        tool_call_id: &str,
        snapshot: &CommandSessionSnapshot,
    ) -> String {
        let display_state = if snapshot.status == CommandSessionStatus::Failed {
            "failed"
        } else {
            "completed"
        };
        self.push_visible_session_record_message_with_metadata(
            session_id,
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            human_tool_display_title("shell_session", display_state),
            build_command_session_stop_result_body(snapshot),
            build_command_session_stop_result_metadata(tool_call_id, snapshot),
            false,
        )
    }

    /// Advances the centered thinking modal animation and removes the modal once the closing phase completes.
    /// 推进居中思考弹层动画，并在关闭阶段完成后移除该弹层。
    pub(super) fn advance_thinking_modal_animation(&mut self) {
        let Some(modal_state) = self.active_thinking_modal.as_mut() else {
            return;
        };

        if modal_state.started_at.elapsed() < THINKING_MODAL_ANIMATION_DURATION {
            return;
        }

        match modal_state.phase {
            ThinkingModalPhase::Opening => {
                modal_state.phase = ThinkingModalPhase::Open;
                modal_state.started_at = Instant::now();
            }
            ThinkingModalPhase::Open => {}
            ThinkingModalPhase::Closing => {
                self.active_thinking_modal = None;
            }
        }
    }

    /// Advances the centered tool modal animation and removes the modal once the closing phase completes.
    /// 推进居中工具弹层动画，并在关闭阶段完成后移除该弹层。
    pub(super) fn advance_tool_modal_animation(&mut self) {
        let Some(modal_state) = self.active_tool_modal.as_mut() else {
            return;
        };

        if modal_state.started_at.elapsed() < THINKING_MODAL_ANIMATION_DURATION {
            return;
        }

        match modal_state.phase {
            ThinkingModalPhase::Opening => {
                modal_state.phase = ThinkingModalPhase::Open;
                modal_state.started_at = Instant::now();
            }
            ThinkingModalPhase::Open => {}
            ThinkingModalPhase::Closing => {
                self.active_tool_modal = None;
            }
        }
    }
}

//! Session transcript runtime and canonical conversation helpers for the main workbench.
//! 主工作台的会话转录运行态与规范会话辅助逻辑。

use super::*;
use vulcan_context::{ContextBudgetSeverity, observed_input_tokens_from_usage};
use vulcan_core::types::ProviderUsageSnapshot;

impl VulcanWorkbench {
    /// Pushes a new activity entry while keeping the most recent items first.
    /// 追加新的活动日志项，并保持最新记录位于最前面。
    pub(super) fn push_activity(
        &mut self,
        level: ActivityLevel,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let entry = ActivityEntry::new(level, title, detail);
        let buffer = text_editor::Content::with_text(&entry.detail);
        // Insert at the front so the latest orchestration feedback remains visible immediately.
        // 采用头插方式，让最新的编排反馈可以被立即看到。
        self.activities.insert(0, entry);
        self.activity_raw_buffers.insert(0, buffer);

        // Trim the log to a manageable size so the prototype stays readable.
        // 将日志修剪到可管理范围内，保证原型界面持续可读。
        if self.activities.len() > 8 {
            self.activities.truncate(8);
        }
        if self.activity_raw_buffers.len() > 8 {
            self.activity_raw_buffers.truncate(8);
        }
    }

    /// Appends one transcript message to the target session while preserving stable insertion order.
    /// 向目标会话追加一条转录消息，并保持稳定插入顺序。
    pub(super) fn push_session_message(
        &mut self,
        session_id: SessionId,
        kind: SessionMessageKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> String {
        self.append_session_message_entry(session_id, SessionMessageEntry::new(kind, title, body))
    }

    /// Appends one hidden internal message node to the target session while preserving stable insertion order.
    /// 向目标会话追加一条隐藏内部消息节点，并保持稳定插入顺序。
    pub(super) fn push_hidden_session_message(
        &mut self,
        session_id: SessionId,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
        metadata: Option<Value>,
    ) -> String {
        self.append_session_message_entry(
            session_id,
            SessionMessageEntry {
                id: String::new(),
                kind,
                record_kind,
                visible: false,
                title: title.into(),
                body: body.into(),
                metadata,
                created_at: String::new(),
            },
        )
    }

    /// Appends one visible transcript message with an explicit durable record kind and optional default-collapsed state.
    /// 追加一条带显式持久化记录类型且可选默认折叠状态的可见转录消息。
    pub(super) fn push_visible_session_record_message(
        &mut self,
        session_id: SessionId,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
        collapsed_by_default: bool,
    ) -> String {
        self.push_visible_session_record_message_with_metadata(
            session_id,
            kind,
            record_kind,
            title,
            body,
            None,
            collapsed_by_default,
        )
    }

    /// Appends one visible transcript message together with optional structured metadata used only by transcript projections.
    /// 追加一条带可选结构化元数据的可见转录消息，且该元数据仅供转录投影使用。
    pub(super) fn push_visible_session_record_message_with_metadata(
        &mut self,
        session_id: SessionId,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
        metadata: Option<Value>,
        collapsed_by_default: bool,
    ) -> String {
        let title = title.into();
        let body = body.into();
        let message_id = self.append_session_message_entry(
            session_id,
            SessionMessageEntry {
                id: String::new(),
                kind,
                record_kind,
                visible: true,
                title,
                body,
                metadata,
                created_at: String::new(),
            },
        );

        if collapsed_by_default {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state
                .ui_state
                .collapsed_message_ids
                .insert(message_id.clone());
            runtime_state.updated_at = current_unix_millis_text();
        }

        message_id
    }

    /// Materializes one session message entry with stable runtime metadata and appends it to the transcript map.
    /// 使用稳定运行时元数据物化一条会话消息项并将其追加到转录映射中。
    pub(super) fn append_session_message_entry(
        &mut self,
        session_id: SessionId,
        entry: SessionMessageEntry,
    ) -> String {
        self.ensure_session_conversation_state_loaded(session_id);
        let conversation_record_count = self
            .session_runtime_state(session_id)
            .map(|runtime_state| runtime_state.conversation_state.records.len())
            .unwrap_or(0);
        let created_at = current_unix_millis_text();
        let message_id =
            build_session_record_message_id(session_id, &created_at, conversation_record_count);
        let materialized_entry = entry.with_runtime_metadata(message_id.clone(), created_at);
        let projection_entry = compact_tool_detail_message_for_projection(&materialized_entry);
        self.dirty_session_message_ids.insert(session_id);
        self.append_session_record_to_conversation(session_id, &materialized_entry);
        self.remember_tool_detail_message(session_id, &materialized_entry);
        if materialized_entry.visible {
            self.session_message_raw_buffers.insert(
                message_id.clone(),
                text_editor::Content::with_text(&projection_entry.body),
            );
            let (markdown_content, markdown_buffer) =
                parse_selectable_markdown_body(&projection_entry.body, &self.app_theme);
            self.session_message_markdown
                .insert(message_id.clone(), markdown_content);
            self.session_message_markdown_buffers
                .insert(message_id.clone(), markdown_buffer);
            self.sync_session_messages_from_conversation_state(session_id);
            self.refresh_session_agent_view_buffers(session_id);
        }
        message_id
    }

    /// Returns the immutable runtime-state snapshot for one visible or archived session when it exists.
    /// 返回某个可见或归档会话在存在时对应的只读运行时状态快照。
    pub(super) fn session_runtime_state(
        &self,
        session_id: SessionId,
    ) -> Option<&SessionRuntimeStateEntry> {
        self.session_runtime_state
            .get(&session_id)
            .or_else(|| self.archived_session_runtime_state.get(&session_id))
    }

    /// Returns the immutable transcript message entry for one visible or archived session/message pair when it exists.
    /// 返回某个可见或归档会话/消息对在存在时对应的只读转录消息条目。
    pub(super) fn session_message_entry(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<&SessionMessageEntry> {
        self.session_messages
            .get(&session_id)
            .or_else(|| self.archived_session_messages.get(&session_id))?
            .iter()
            .find(|message| message.id == message_id)
    }

    /// Stores every full tool-detail row from a message batch in the out-of-band detail cache.
    /// 将一批消息中的完整工具明细行保存到带外详情缓存。
    pub(super) fn remember_tool_detail_messages(
        &mut self,
        session_id: SessionId,
        messages: &[SessionMessageEntry],
    ) {
        for message in messages {
            self.remember_tool_detail_message(session_id, message);
        }
    }

    /// Stores one full tool-detail row outside the main UI projection when it carries hidden payload text.
    /// 当单条工具明细行携带隐藏载荷文本时，将其保存到主界面投影之外。
    pub(super) fn remember_tool_detail_message(
        &mut self,
        session_id: SessionId,
        message: &SessionMessageEntry,
    ) {
        if !session_message_is_tool_detail(message) {
            return;
        }
        self.session_tool_detail_messages
            .entry(session_id)
            .or_default()
            .insert(message.id.clone(), message.clone());
    }

    /// Returns the full tool-detail row for one session/message pair when it has been loaded or produced locally.
    /// 当指定会话/消息对已经加载或本地产生时，返回对应完整工具明细行。
    pub(super) fn full_tool_detail_message_entry(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> Option<&SessionMessageEntry> {
        self.session_tool_detail_messages
            .get(&session_id)?
            .get(message_id)
    }

    /// Builds the full modal-facing tool row, merging a matching result back into its call row when the visible card is merged.
    /// 构建面向弹层的完整工具行；当可见卡片已合并时，将匹配的返回结果重新合并到调用行中。
    pub(super) fn full_tool_modal_message_entry(
        &self,
        session_id: SessionId,
        message: &SessionMessageEntry,
    ) -> SessionMessageEntry {
        let mut full_message = self
            .full_tool_detail_message_entry(session_id, &message.id)
            .cloned()
            .unwrap_or_else(|| message.clone());
        if full_message.record_kind != SessionMessageRecordKind::ToolCallOutput {
            return full_message;
        }

        let Some(tool_call_id) = tool_display_message_call_id(&full_message) else {
            return full_message;
        };
        let Some(result_message) = self
            .session_tool_detail_messages
            .get(&session_id)
            .and_then(|messages| {
                messages.values().find(|candidate| {
                    candidate.record_kind == SessionMessageRecordKind::ToolResultOutput
                        && tool_display_message_call_id(candidate).as_deref()
                            == Some(tool_call_id.as_str())
                })
            })
            .cloned()
        else {
            return full_message;
        };

        merge_tool_result_into_display_entry(&mut full_message, result_message);
        full_message
    }

    /// Returns the session identifier that currently owns the visible transcript surface, whether it is the root session or one opened subagent panel.
    /// 返回当前可见转录表面所属的会话标识，无论该表面属于根会话还是已打开的子代理面板。
    pub(super) fn active_transcript_surface_session_id(&self) -> Option<SessionId> {
        self.selected_subagent_task_panel_state()
            .map(|state| state.task_id)
            .or_else(|| self.selected_session().map(|session| session.id))
    }

    /// Returns whether one session currently owns the visible transcript surface and therefore warrants live view-cache refresh work.
    /// 返回某个会话当前是否拥有可见转录表面，并因此值得承担实时视图缓存刷新开销。
    pub(super) fn session_owns_active_transcript_surface(&self, session_id: SessionId) -> bool {
        self.active_transcript_surface_session_id() == Some(session_id)
    }

    /// Invalidates heavyweight transcript projection caches for one hidden session so background streaming keeps the data fresh without rebuilding invisible views.
    /// 使某个隐藏会话的重型转录投影缓存失效，让后台流式输出仅保持数据新鲜，而不重建不可见视图。
    pub(super) fn invalidate_hidden_session_streaming_view_caches(
        &mut self,
        session_id: SessionId,
        message_id: &str,
    ) {
        if !message_id.is_empty() {
            self.session_message_raw_buffers.remove(message_id);
            self.session_message_markdown.remove(message_id);
            self.session_message_markdown_buffers.remove(message_id);
        }
        self.loading_session_agent_view_projection_ids
            .remove(&session_id);
        self.collapsed_runtime_event_ids.remove(&session_id);
        self.markdown_runtime_event_ids.remove(&session_id);
        self.remove_runtime_event_raw_buffers(session_id);
        self.clear_session_agent_view_text_buffers(session_id);
        self.session_agent_view_documents.remove(&session_id);
        self.clear_session_inline_agent_view_document_cache(session_id);
    }

    /// Appends one canonical record to the session standard state so transcript projection never becomes the source of truth.
    /// 向会话标准状态追加一条规范记录，确保转录投影永远不是事实源。
    pub(super) fn append_session_record_to_conversation(
        &mut self,
        session_id: SessionId,
        entry: &SessionMessageEntry,
    ) {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: entry.id.clone(),
                kind: entry.kind,
                record_kind: entry.record_kind,
                visible: entry.visible,
                title: entry.title.clone(),
                body: entry.body.clone(),
                metadata: entry.metadata.clone(),
                created_at: entry.created_at.clone(),
            },
        );
        runtime_state.updated_at = entry.created_at.clone();
    }

    /// Rebuilds one session transcript projection from canonical records and stores it in the visible or archived cache bucket.
    /// 根据规范记录重建单个会话的转录投影，并将其存入可见或归档缓存桶。
    pub(super) fn sync_session_messages_from_conversation_state(&mut self, session_id: SessionId) {
        let full_projected_messages = self
            .session_runtime_state(session_id)
            .map(|state| conversation_state_to_session_messages(&state.conversation_state))
            .unwrap_or_default();
        self.remember_tool_detail_messages(session_id, &full_projected_messages);
        let projected_messages = full_projected_messages
            .iter()
            .map(compact_tool_detail_message_for_projection)
            .collect::<Vec<_>>();
        let session_is_archived = self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id);
        let existing_messages = if session_is_archived {
            self.archived_session_messages
                .get(&session_id)
                .cloned()
                .unwrap_or_default()
        } else {
            self.session_messages
                .get(&session_id)
                .cloned()
                .unwrap_or_default()
        };
        let merged_messages = merge_projected_session_messages_into_existing_window(
            &existing_messages,
            projected_messages,
        );

        if session_is_archived {
            self.session_messages.remove(&session_id);
            if merged_messages.is_empty() {
                self.archived_session_messages.remove(&session_id);
            } else {
                self.archived_session_messages
                    .insert(session_id, merged_messages);
            }
        } else {
            self.archived_session_messages.remove(&session_id);
            if merged_messages.is_empty() {
                self.session_messages.remove(&session_id);
            } else {
                self.session_messages.insert(session_id, merged_messages);
            }
        }
        self.rebuild_subagent_task_cache_for_session(session_id);
    }

    /// Appends one streamed body delta directly into the already projected visible transcript row so live rendering does not need to rebuild the whole session message list first.
    /// 直接将一段流式正文增量追加到已投影的可见转录行中，避免实时渲染先重建整份会话消息列表。
    pub(super) fn append_stream_delta_to_projected_session_message(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) -> bool {
        if let Some(messages) = self.session_messages.get_mut(&session_id)
            && let Some(message) = messages.iter_mut().find(|message| message.id == message_id)
        {
            message.body.push_str(delta);
            return true;
        }

        if let Some(messages) = self.archived_session_messages.get_mut(&session_id)
            && let Some(message) = messages.iter_mut().find(|message| message.id == message_id)
        {
            message.body.push_str(delta);
            return true;
        }

        false
    }

    /// Keeps streamed message projection caches untouched on the hot path so large read-only buffers are not dropped on every token.
    /// 在流式热路径上保持消息投影缓存不动，避免每个 token 都释放大型只读缓冲对象。
    pub(super) fn refresh_streamed_session_message_view_caches(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) {
        if delta.is_empty() {
            return;
        }

        let _ = session_id;
        let _ = message_id;
    }

    /// Appends one streamed body delta into an existing visible transcript message without rebuilding legacy per-message Markdown caches.
    /// 将一段流式正文增量追加到现有可见转录消息，并同步刷新逐消息 Markdown 投影缓存。
    pub(super) fn append_stream_delta_to_session_message(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        delta: &str,
    ) {
        let mut perf_trace = TranscriptPerfTrace::begin(
            "append_stream_delta_to_session_message",
            format!(
                "session_id={session_id} message_id={message_id} delta_chars={}",
                delta.chars().count()
            ),
        );
        self.ensure_session_conversation_state_loaded(session_id);
        let mut phase_started_at = Instant::now();
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        if !append_record_body_delta(&mut runtime_state.conversation_state, message_id, delta) {
            perf_trace.checkpoint("append_record_body_delta", phase_started_at);
            perf_trace.finish();
            return;
        }
        perf_trace.checkpoint("append_record_body_delta", phase_started_at);
        phase_started_at = Instant::now();
        runtime_state.updated_at = current_unix_millis_text();
        self.dirty_session_message_ids.insert(session_id);
        perf_trace.checkpoint("mark_session_dirty", phase_started_at);
        phase_started_at = Instant::now();
        if !self.append_stream_delta_to_projected_session_message(session_id, message_id, delta) {
            self.sync_session_messages_from_conversation_state(session_id);
        }
        perf_trace.checkpoint("project_visible_messages", phase_started_at);
        phase_started_at = Instant::now();
        if self.session_owns_active_transcript_surface(session_id) {
            self.refresh_streamed_session_message_view_caches(session_id, message_id, delta);
            perf_trace.checkpoint("refresh_message_view_caches", phase_started_at);
            phase_started_at = Instant::now();
            if !self.refresh_session_agent_view_message_blocks(session_id, message_id) {
                self.refresh_session_agent_view_buffers(session_id);
            }
        } else {
            self.invalidate_hidden_session_streaming_view_caches(session_id, message_id);
            perf_trace.checkpoint("refresh_message_view_caches", phase_started_at);
            phase_started_at = Instant::now();
        }
        perf_trace.checkpoint("refresh_agent_view", phase_started_at);
        perf_trace.finish();
    }

    /// Ensures the session owns one visible thinking message and appends the incoming delta to it.
    /// 确保会话拥有一条可见思考消息，并将当前增量追加进去。
    pub(super) fn append_streaming_thinking_delta(&mut self, session_id: SessionId, delta: &str) {
        let message_id =
            if let Some(message_id) = self.streaming_thinking_message_ids.get(&session_id) {
                message_id.clone()
            } else {
                let message_id = self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ThinkingOutput,
                    "思考",
                    String::new(),
                    true,
                );
                self.streaming_thinking_message_ids
                    .insert(session_id, message_id.clone());
                message_id
            };

        self.floating_streaming_thinking_sessions.insert(session_id);
        self.session_streaming_thinking_follow_tail
            .entry(session_id)
            .or_insert(true);
        self.append_stream_delta_to_session_message(session_id, &message_id, delta);
    }

    /// Docks the current streaming thinking message back into the main transcript while keeping subsequent deltas on the same message.
    /// 将当前流式思考消息收回主转录正文中，同时保留后续增量继续写入同一条消息。
    pub(super) fn dock_streaming_thinking_message(&mut self, session_id: SessionId) {
        self.clear_thinking_overlay_drag_auto_scroll(session_id);
        self.floating_streaming_thinking_sessions
            .remove(&session_id);
        self.session_streaming_thinking_follow_tail
            .remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Ensures the session owns one visible final-output message and appends the incoming delta to it.
    /// 确保会话拥有一条可见正式输出消息，并将当前增量追加进去。
    pub(super) fn append_streaming_output_delta(&mut self, session_id: SessionId, delta: &str) {
        let message_id =
            if let Some(message_id) = self.streaming_output_message_ids.get(&session_id) {
                message_id.clone()
            } else {
                let message_id = self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::AssistantOutput,
                    "输出",
                    String::new(),
                    false,
                );
                self.streaming_output_message_ids
                    .insert(session_id, message_id.clone());
                message_id
            };

        self.append_stream_delta_to_session_message(session_id, &message_id, delta);
    }

    /// Appends one hidden derived session record produced by the dedicated session skill chain.
    /// 追加一条由独立会话技能链产出的隐藏派生会话记录。
    pub(super) fn push_session_derived_record(
        &mut self,
        session_id: SessionId,
        record: SessionDerivedRecord,
    ) -> String {
        let record_kind = record.record_kind;
        let body = record.body.clone();
        let message_id = self.push_hidden_session_message(
            session_id,
            record.kind,
            record.record_kind,
            record.title,
            record.body,
            Some(json!({
                "preview": record.preview,
            })),
        );
        self.maybe_mirror_hidden_record_to_session_conversation(
            session_id,
            record_kind,
            &message_id,
            &body,
        );
        message_id
    }

    /// Ensures the target session has one canonical conversation snapshot owned by the runtime-state layer.
    /// 确保目标会话拥有一份由运行时状态层持有的规范会话快照。
    pub(super) fn ensure_session_conversation_state_loaded(&mut self, session_id: SessionId) {
        let _ = self.ensure_session_runtime_state_mut(session_id);
    }

    /// Returns the full canonical conversation state that should be forwarded into the next runtime call.
    /// 返回应转发给下一次运行时调用的完整规范会话状态。
    pub(super) fn session_conversation_state(
        &self,
        session_id: SessionId,
    ) -> crate::conversation_protocol::SessionConversationState {
        self.session_runtime_state
            .get(&session_id)
            .map(|state| state.conversation_state.clone())
            .unwrap_or_default()
    }

    /// Appends the latest user prompt to the canonical conversation state before the real runtime starts.
    /// 在真实运行时启动前，将最新用户提示追加到规范会话状态。
    pub(super) fn append_user_prompt_to_session_conversation(
        &mut self,
        session_id: SessionId,
        prompt: String,
        attachments: Vec<ConversationAttachment>,
        promoted_staging_scopes: Vec<String>,
    ) -> Result<(), String> {
        self.ensure_session_conversation_state_loaded(session_id);
        let parts = build_composer_input_parts(&prompt, &attachments);
        let (canonical_text, canonical_attachments) = composer_parts_to_user_payload(&parts);
        let protocol_message_id = format!(
            "session-{session_id}-protocol-user-{}",
            current_unix_millis_text()
        );
        let updated_at = current_unix_millis_text();
        let referenced_attachments = canonical_attachments.clone();
        let attachment_workspace_path = self.attachment_workspace_path_for_session(session_id);
        crate::attachment_asset_index::promote_staged_attachment_to_message_ref(
            &attachment_workspace_path,
            &session_id.to_string(),
            &protocol_message_id,
            &referenced_attachments,
            &promoted_staging_scopes,
        )
        .map_err(|error| format!("更新附件资产引用索引失败：{error}"))?;
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            append_user_text_message(
                &mut runtime_state.conversation_state,
                protocol_message_id.clone(),
                canonical_text,
                canonical_attachments,
            );
            runtime_state.updated_at = updated_at;
        }
        Ok(())
    }

    /// Mirrors selected hidden transcript records into the canonical conversation state so existing derived context remains effective.
    /// 将选定的隐藏转录记录镜像到规范会话状态，从而保留既有派生上下文效果。
    pub(super) fn maybe_mirror_hidden_record_to_session_conversation(
        &mut self,
        session_id: SessionId,
        record_kind: SessionMessageRecordKind,
        message_id: &str,
        body: &str,
    ) {
        let should_mirror = matches!(record_kind, SessionMessageRecordKind::ExploreOutput);
        if !should_mirror {
            return;
        }

        self.ensure_session_conversation_state_loaded(session_id);
        let updated_at = current_unix_millis_text();
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            message_id.to_string(),
            body.to_string(),
        );
        runtime_state.updated_at = updated_at;
    }

    /// Rewrites the canonical conversation window after one transcript compaction summary is created.
    /// 在生成一条转录压缩摘要后改写规范会话窗口。
    pub(super) fn compact_session_conversation_history(
        &mut self,
        session_id: SessionId,
        summary_message_id: &str,
        summary_kind: SessionMessageKind,
        summary_body: &str,
        created_at: &str,
        policy: ContextCompactionPolicy,
    ) {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        compact_conversation_with_summary(
            &mut runtime_state.conversation_state,
            summary_message_id.to_string(),
            summary_kind,
            summary_body.to_string(),
            created_at.to_string(),
            policy,
        );
        compact_conversation_records_after_summary(
            &mut runtime_state.conversation_state,
            summary_message_id,
            policy,
        );
        runtime_state.updated_at = created_at.to_string();
        self.sync_session_messages_from_conversation_state(session_id);
        self.refresh_session_agent_view_buffers(session_id);
    }

    /// Applies one prepared compaction result to the current session state, persists its checkpoint, and refreshes context usage diagnostics.
    /// 将一份已准备好的压缩结果应用到当前会话状态，持久化其检查点，并刷新上下文占用诊断。
    pub(super) fn apply_session_history_compaction_result(
        &mut self,
        session_id: SessionId,
        compaction_result: CompactionSkillResult,
        activity_title: &str,
        activity_detail: String,
    ) {
        let summary_message_id = self.push_session_derived_record(
            session_id,
            into_session_derived_record(compaction_result.summary_record.clone()),
        );
        let mut checkpoint = compaction_result.materialize_checkpoint(summary_message_id);
        let created_at = checkpoint.created_at.clone();
        let compaction_id = checkpoint.id.clone();
        let context_start_message_id = checkpoint.context_start_message_id.clone();
        let summary_message_id = checkpoint.summary_message_id.clone();
        let checkpoint_budget_fingerprint = checkpoint.context_budget_fingerprint.clone();
        let checkpoint_assembly_fingerprint = checkpoint.context_assembly_fingerprint.clone();
        if let Some(runtime_state_snapshot) = self.session_runtime_state.get(&session_id) {
            checkpoint.context_budget_fingerprint = runtime_state_snapshot
                .live_context_budget_fingerprint
                .clone();
            checkpoint.context_assembly_fingerprint = runtime_state_snapshot
                .live_context_assembly_fingerprint
                .clone();
        }
        let checkpoint_budget_fingerprint = checkpoint
            .context_budget_fingerprint
            .clone()
            .or(checkpoint_budget_fingerprint);
        let checkpoint_assembly_fingerprint = checkpoint
            .context_assembly_fingerprint
            .clone()
            .or(checkpoint_assembly_fingerprint);
        self.session_compactions
            .entry(session_id)
            .or_default()
            .push(checkpoint);
        let runtime_state_updated_at = created_at.clone();
        {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            runtime_state.last_compaction_id = Some(compaction_id);
            runtime_state.context_start_message_id = context_start_message_id;
            runtime_state.anchor_context_budget_fingerprint = checkpoint_budget_fingerprint;
            runtime_state.anchor_context_assembly_fingerprint = checkpoint_assembly_fingerprint;
            runtime_state.updated_at = created_at;
        }
        if let Some(summary_message_id) = summary_message_id {
            self.compact_session_conversation_history(
                session_id,
                &summary_message_id,
                compaction_result.summary_record.kind,
                &compaction_result.summary_record.body,
                &runtime_state_updated_at,
                compaction_result.policy,
            );
        }
        self.push_activity(ActivityLevel::Info, activity_title, activity_detail);
        self.sync_context_usage_from_session_budget(session_id);
    }

    /// Starts one prompt-based session compaction task or immediately falls back to the legacy host-side summary when internal execution prerequisites are unavailable.
    /// 启动一次提示词式会话压缩任务；若内部执行前提缺失，则立即回退到旧版宿主侧摘要。
    pub(super) fn start_session_prompt_compaction(
        &mut self,
        session_id: SessionId,
        trigger: SessionPromptCompactionTrigger,
        plan: CompactionPlan,
        budget_note: String,
    ) -> Task<Message> {
        if self.session_has_active_compaction(session_id) {
            return Task::none();
        }

        let previous_phase = self.session_run_phase_for(session_id);
        let session_title = self
            .session_by_id(session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| format!("会话 #{session_id}"));
        let workspace_path = self
            .workspace_path_for_session_id(session_id)
            .unwrap_or_else(|| self.workspace_path.clone());
        let model_reference = self.preferred_model_reference_for_session_compaction(session_id);
        let provider_configuration =
            self.provider_configuration_for_model_reference(model_reference.as_ref());
        let runtime_options = self.current_pending_prompt_runtime_options();
        let ai_tool_configuration =
            self.effective_ai_tool_configuration_for_runtime_options(&runtime_options);
        let Some(mcp_runtime_access) = self.mcp_runtime.clone() else {
            return self.finish_session_prompt_compaction(
                session_id,
                trigger,
                budget_note,
                previous_phase,
                PromptCompactionExecution {
                    compaction_result: CompactionSkillResult::from_legacy_plan(&plan),
                    fallback_used: true,
                    provider_request_count: 0,
                },
            );
        };

        let request = PromptCompactionJobRequest {
            session_id,
            session_title,
            workspace_path,
            plan: plan.clone(),
            provider_configuration,
            model_reference,
            reasoning_effort: runtime_options.reasoning_effort,
            ai_tool_configuration,
            agent_profile_catalog: self.agent_profile_catalog.clone(),
            luaskills_runtime: self.luaskills_runtime.clone(),
            mcp_runtime_access,
            task_runtime: self.runtime_foundation.task_runtime(),
            question_runtime: self.runtime_foundation.question_runtime(),
            command_session_runtime: self.runtime_foundation.command_session_runtime(),
            token_budget_runtime: self.runtime_foundation.token_budget_runtime(),
        };

        self.set_session_run_phase(session_id, SessionRunPhase::Preparing);
        self.status_message = match trigger {
            SessionPromptCompactionTrigger::ManualSlash => {
                String::from("正在为当前会话生成压缩总结，请稍候。")
            }
            SessionPromptCompactionTrigger::BudgetAuto => {
                String::from("当前会话正在根据上下文预算生成压缩总结，请稍候。")
            }
        };

        let run_id = self.next_run_id;
        self.next_run_id = self.next_run_id.saturating_add(1);
        let (run_task, run_handle) = Task::perform(execute_prompt_compaction_job(request), {
            move |result| Message::SessionPromptCompactionFinished {
                run_id,
                session_id,
                trigger,
                budget_note,
                plan,
                result,
            }
        })
        .abortable();
        self.active_session_compactions.insert(
            session_id,
            ActiveSessionCompactionState {
                run_id,
                handle: run_handle,
                previous_phase,
            },
        );

        run_task
    }

    /// Applies the terminal prompt-compaction result, restores the previous session phase, and refreshes persistence and queue dispatch.
    /// 应用提示词压缩终态结果，恢复先前会话阶段，并刷新持久化与队列派发。
    pub(super) fn finish_session_prompt_compaction(
        &mut self,
        session_id: SessionId,
        trigger: SessionPromptCompactionTrigger,
        budget_note: String,
        previous_phase: SessionRunPhase,
        execution: PromptCompactionExecution,
    ) -> Task<Message> {
        let activity_title = match trigger {
            SessionPromptCompactionTrigger::ManualSlash => "手动触发上下文压缩",
            SessionPromptCompactionTrigger::BudgetAuto => "上下文预算触发压缩",
        };
        let provider_request_note = if execution.provider_request_count > 0 {
            format!(
                " 本次压缩共发起 {} 次 Provider 请求。",
                execution.provider_request_count
            )
        } else {
            String::new()
        };
        let activity_detail = match (trigger, execution.fallback_used) {
            (SessionPromptCompactionTrigger::ManualSlash, false) => {
                format!(
                    "会话 #{session_id} 已通过 slash 指令执行提示词式手动压缩：{budget_note}。{provider_request_note}"
                )
            }
            (SessionPromptCompactionTrigger::ManualSlash, true) => format!(
                "会话 #{session_id} 的提示词式手动压缩已回退到宿主侧摘要：{budget_note}。{provider_request_note}"
            ),
            (SessionPromptCompactionTrigger::BudgetAuto, false) => {
                format!(
                    "会话 #{session_id} 已根据预算快照执行提示词式压缩：{budget_note}。{provider_request_note}"
                )
            }
            (SessionPromptCompactionTrigger::BudgetAuto, true) => format!(
                "会话 #{session_id} 的提示词式预算压缩已回退到宿主侧摘要：{budget_note}。{provider_request_note}"
            ),
        };
        self.apply_session_history_compaction_result(
            session_id,
            execution.compaction_result,
            activity_title,
            activity_detail,
        );
        self.set_session_run_phase(session_id, previous_phase);
        self.status_message = match (trigger, execution.fallback_used) {
            (SessionPromptCompactionTrigger::ManualSlash, false) => {
                format!("当前会话已执行手动上下文压缩：{budget_note}。")
            }
            (SessionPromptCompactionTrigger::ManualSlash, true) => format!(
                "当前会话已执行手动上下文压缩，但模型总结失败，已回退到宿主侧摘要：{budget_note}。"
            ),
            (SessionPromptCompactionTrigger::BudgetAuto, false) => {
                format!("当前会话已根据预算完成上下文压缩：{budget_note}。")
            }
            (SessionPromptCompactionTrigger::BudgetAuto, true) => format!(
                "当前会话已根据预算完成上下文压缩，但模型总结失败，已回退到宿主侧摘要：{budget_note}。"
            ),
        };

        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
            self.maybe_start_next_pending_prompt(session_id, false),
        ])
    }

    /// Tries to compact the current selected session through the prompt-based summary pipeline when the user invokes the slash command manually.
    /// 当用户手动调用 slash 指令时，通过提示词式总结链路尝试压缩当前选中会话。
    pub(super) fn execute_manual_selected_session_history_compaction(&mut self) -> Task<Message> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            self.status_message = String::from("当前没有可执行手动压缩的会话。");
            return operation::focus(COMPOSER_EDITOR_ID);
        };
        if self.selected_session_has_busy_execution_lane() {
            self.status_message =
                String::from("当前会话仍有运行中的执行链路，暂时无法执行手动上下文压缩。");
            return operation::focus(COMPOSER_EDITOR_ID);
        }

        let messages = self.session_context_history(session_id);
        if messages.is_empty() {
            self.status_message = String::from("当前会话暂时没有可压缩的历史上下文。");
            return operation::focus(COMPOSER_EDITOR_ID);
        }

        let runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let full_session_messages =
            conversation_state_to_session_messages(&runtime_state_snapshot.conversation_state);
        let context_budget = self.resolve_session_context_budget_snapshot(
            session_id,
            &runtime_state_snapshot,
            &messages,
            false,
        );
        let Some(plan) = plan_compaction(CompactionSkillInput {
            session_label: &session_id.to_string(),
            messages: &messages,
            all_messages: &full_session_messages,
            context_start_message_id: runtime_state_snapshot.context_start_message_id.as_deref(),
            policy: ContextCompactionPolicy::default(),
        }) else {
            self.sync_context_usage_from_session_budget(session_id);
            self.status_message = String::from("当前会话暂时没有可压缩的历史上下文。");
            return operation::focus(COMPOSER_EDITOR_ID);
        };

        let budget_note = context_budget
            .as_ref()
            .map(ContextBudgetSnapshot::summary_note)
            .unwrap_or_else(|| String::from("当前未提供预算快照"));
        Task::batch([
            self.start_session_prompt_compaction(
                session_id,
                SessionPromptCompactionTrigger::ManualSlash,
                plan,
                budget_note,
            ),
            operation::focus(COMPOSER_EDITOR_ID),
        ])
    }

    /// Returns the mutable runtime-state snapshot for the target session, creating one default row on demand.
    /// 返回目标会话对应的可变运行时状态快照，并在缺失时按需创建默认行。
    pub(super) fn ensure_session_runtime_state_mut(
        &mut self,
        session_id: SessionId,
    ) -> &mut SessionRuntimeStateEntry {
        self.hydrated_session_ids.insert(session_id);
        self.dirty_session_payload_ids.insert(session_id);
        self.session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            })
    }

    /// Writes one hidden explore output for the target session so project-scoped context survives persistence.
    /// 为目标会话写入一条隐藏探索输出，从而让项目级上下文在持久化后仍可恢复。
    pub(super) fn append_session_explore_output(
        &mut self,
        session_id: SessionId,
        project_id: Option<&str>,
    ) {
        let derived_record = {
            let project = project_id
                .and_then(|resolved_project_id| self.task_project_by_id(resolved_project_id));
            self.session_skill_chain
                .run_explore_skill(ExploreSkillInput {
                    workspace_path: &self.workspace_path,
                    project_id,
                    project,
                })
        };
        self.push_session_derived_record(session_id, derived_record);
    }

    /// Generates a new session title from the latest user prompt when the session title is still auto-managed.
    /// 当会话标题仍处于自动管理状态时，根据最新用户提示词生成新的会话标题。
    pub(super) fn maybe_generate_session_title_from_prompt(
        &mut self,
        session_id: SessionId,
        prompt: &str,
    ) -> Option<String> {
        let (current_title, current_title_source) = self
            .session_by_id(session_id)
            .map(|session| (session.title.clone(), session.title_source))
            .unwrap_or_else(|| (String::new(), SessionTitleSource::Generated));
        let result = self.session_skill_chain.run_title_agent(TitleAgentInput {
            current_title: &current_title,
            title_source: current_title_source,
            prompt,
        })?;

        if let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = result.generated_title.clone();
            session.title_source = SessionTitleSource::Generated;
            session.last_updated_label = String::from("刚刚");
        }
        self.push_session_derived_record(session_id, result.hidden_record);

        Some(result.generated_title)
    }

    /// Counts canonical user-authored turns currently stored on the session protocol state.
    /// 统计当前会话协议状态中已保存的规范用户轮次数量。
    pub(super) fn session_user_turn_count(&self, session_id: SessionId) -> usize {
        self.session_runtime_state(session_id)
            .map(|runtime_state| {
                runtime_state
                    .conversation_state
                    .messages
                    .iter()
                    .filter(|message| {
                        matches!(
                            message.payload,
                            crate::conversation_protocol::SessionConversationPayload::UserText {
                                ..
                            }
                        )
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Appends one hidden summary output after a real execution run completes.
    /// 在一次真实执行完成后追加一条隐藏摘要输出。
    pub(super) fn append_session_summary_output(
        &mut self,
        session_id: SessionId,
        assistant_output: &str,
    ) -> String {
        let report = ExecutionRunReport {
            session_id,
            preparation: None,
            assistant_output: Some(assistant_output.to_string()),
            thinking_output: None,
            conversation_state: None,
            provider_request_count: 0,
            last_successful_provider_usage: None,
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
        };
        let derived_record = self
            .session_skill_chain
            .run_summary_skill(SummarySkillInput { report: &report });
        self.push_session_derived_record(session_id, derived_record)
    }

    /// Appends one hidden terminated-turn marker after an interrupted, stopped, or failed execution branch closes without a summary.
    /// 在一次被打断、被停止或失败且没有摘要的执行分支结束后追加隐藏终止轮次标记。
    pub(super) fn append_session_turn_terminated(
        &mut self,
        session_id: SessionId,
        body: impl Into<String>,
    ) -> String {
        self.push_session_derived_record(
            session_id,
            SessionDerivedRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TurnTerminated,
                title: String::from("Turn Terminated"),
                body: body.into(),
                preview: String::new(),
            },
        )
    }

    /// Aggregates one terminal per-turn statistics snapshot from the current transcript slice and appends a visible lightweight statistics row.
    /// 从当前转录切片聚合一份终态按轮次统计快照，并追加一条可见轻量统计行。
    pub(super) fn append_session_turn_stats(
        &mut self,
        session_id: SessionId,
        closing_record_id: &str,
        status: SessionTurnStatsStatus,
        provider_request_count_override: Option<usize>,
    ) -> Option<String> {
        let conversation_state = self
            .session_runtime_state(session_id)
            .map(|runtime_state| runtime_state.conversation_state.clone())?;
        let turn_stats = build_session_turn_stats_from_state(
            &conversation_state,
            closing_record_id,
            status,
            provider_request_count_override,
        )?;
        let metadata = serde_json::to_value(json!({
            "turn_stats": turn_stats,
            "turn_closing_record_id": closing_record_id,
        }))
        .ok();
        let title = format!("Turn Stats · {}", turn_stats.status.label());
        let body = turn_stats.detail_body();
        Some(self.push_visible_session_record_message_with_metadata(
            session_id,
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::TurnStats,
            title,
            body,
            metadata,
            false,
        ))
    }

    /// Returns the latest safe provider-request count for a manually stopped turn.
    /// 返回手动停止轮次可使用的最近安全 Provider 请求次数。
    pub(super) fn safe_provider_request_count_for_stopped_turn(
        &self,
        session_id: SessionId,
    ) -> Option<usize> {
        self.session_runtime_state(session_id)
            .and_then(|runtime_state| runtime_state.last_safe_provider_request_count)
            .filter(|count| *count > 0)
    }

    /// Updates the runtime-state snapshot for the session from the latest execution preparation.
    /// 根据最近一次执行准备结果更新该会话的运行时状态快照。
    pub(super) fn update_session_runtime_identity_from_preparation(
        &mut self,
        session_id: SessionId,
        preparation: &ExecutionPreparation,
    ) {
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        runtime_state.active_agent_id = Some(preparation.agent_id.clone());
        runtime_state.active_provider_id = Some(preparation.model_blueprint.provider_id.clone());
        runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
        runtime_state.active_openrouter_endpoint_route = preparation
            .model_blueprint
            .openrouter_endpoint_route
            .clone();
        runtime_state.live_context_budget_fingerprint = preparation
            .context_budget
            .as_ref()
            .map(|context_budget| context_budget.fingerprint.clone());
        runtime_state.live_context_assembly_fingerprint = preparation
            .context_baseline
            .as_ref()
            .map(|context_baseline| context_baseline.fingerprint.clone());
        runtime_state.updated_at = current_unix_millis_text();
    }

    /// Synchronizes the visible workbench context bar from the latest execution preparation snapshot.
    /// 根据最近一次执行准备快照同步可见工作台上下文条。
    pub(super) fn sync_context_usage_from_preparation(
        &mut self,
        preparation: Option<&ExecutionPreparation>,
    ) {
        let Some(preparation) = preparation else {
            self.context_usage = 0.0;
            self.context_usage_detail = String::from("等待下一次真实请求预算");
            self.visible_context_budget_snapshot = None;
            return;
        };

        if let Some(context_budget) = preparation.context_budget.as_ref() {
            self.context_usage = context_budget.usage_percent();
            self.context_usage_detail = context_budget.summary_note();
            self.visible_context_budget_snapshot = Some(context_budget.clone());
        } else {
            self.context_usage = 0.0;
            self.context_usage_detail = String::from("等待下一次真实请求预算");
            self.visible_context_budget_snapshot = None;
        }
    }

    /// Resolves the latest request-facing context-budget snapshot captured from a real provider request.
    /// 解析最近一次真实 Provider 请求捕获到的面向请求上下文预算快照。
    pub(super) fn resolve_session_context_budget_snapshot(
        &self,
        session_id: SessionId,
        runtime_state_snapshot: &SessionRuntimeStateEntry,
        _messages: &[SessionMessageEntry],
        validate_anchor_baseline: bool,
    ) -> Option<ContextBudgetSnapshot> {
        let preparation = self
            .last_execution_preparation
            .as_ref()
            .filter(|preparation| preparation.session_id == session_id)?;
        let mut context_budget = preparation.context_budget.clone()?;
        let has_anchor_baseline = runtime_state_snapshot.last_compaction_id.is_some()
            || runtime_state_snapshot.context_start_message_id.is_some()
            || runtime_state_snapshot
                .anchor_context_budget_fingerprint
                .is_some()
            || runtime_state_snapshot
                .anchor_context_assembly_fingerprint
                .is_some();
        if validate_anchor_baseline && has_anchor_baseline {
            context_budget = apply_context_budget_anchor_baseline(
                context_budget,
                runtime_state_snapshot
                    .anchor_context_budget_fingerprint
                    .as_deref(),
                runtime_state_snapshot
                    .anchor_context_assembly_fingerprint
                    .as_deref(),
                runtime_state_snapshot
                    .live_context_assembly_fingerprint
                    .as_deref(),
            );
        }

        Some(context_budget)
    }

    /// Builds one historical context-usage snapshot from the last persisted provider usage when no live request budget is available.
    /// 当没有实时请求预算可用时，根据最近一次持久化 Provider 用量构建历史上下文占用快照。
    fn historical_context_budget_snapshot_from_runtime_state(
        &self,
        runtime_state_snapshot: &SessionRuntimeStateEntry,
    ) -> Option<ContextBudgetSnapshot> {
        let observed_usage = runtime_state_snapshot
            .last_successful_provider_usage
            .as_ref()
            .or(runtime_state_snapshot.last_provider_usage.as_ref())
            .filter(|usage| usage.has_any_tokens())?;
        let provider_id = runtime_state_snapshot.active_provider_id.as_deref()?;
        let model_id = runtime_state_snapshot.active_model_id.as_deref()?;
        let capabilities = self.resolve_runtime_state_model_capabilities(runtime_state_snapshot);
        let effective_input_limit = capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.input_limit.or(capabilities.context_limit));
        let input_tokens = observed_input_tokens_from_usage(Some(observed_usage))
            .or(observed_usage.total_tokens)
            .unwrap_or_default();
        let mut usage_ratio = effective_input_limit
            .map(|limit| input_tokens as f32 / limit.max(1) as f32)
            .unwrap_or(0.0);
        if effective_input_limit.is_none() && input_tokens > 0 {
            usage_ratio = 0.01;
        }

        Some(ContextBudgetSnapshot {
            fingerprint: runtime_state_snapshot
                .live_context_budget_fingerprint
                .clone()
                .unwrap_or_else(|| format!("historical:{provider_id}:{model_id}")),
            context_limit: capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.context_limit),
            effective_input_limit,
            reserved_output_tokens: capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.output_limit)
                .unwrap_or_default(),
            reserved_reasoning_tokens: 0,
            reserved_compaction_tokens: 0,
            compaction_auto_enabled: false,
            baseline_matches_active_runtime: true,
            baseline_budget_fingerprint_matches: true,
            baseline_assembly_fingerprint_matches: true,
            baseline_mismatch_reason: None,
            estimated_system_tokens: 0,
            estimated_history_tokens: input_tokens,
            estimated_prompt_tokens: 0,
            estimated_tool_tokens: 0,
            estimated_input_tokens: input_tokens,
            estimated_total_tokens: observed_usage
                .total_tokens
                .unwrap_or_else(|| {
                    input_tokens
                        .saturating_add(observed_usage.output_tokens.unwrap_or_default())
                        .saturating_add(observed_usage.reasoning_tokens.unwrap_or_default())
                }),
            usage_ratio,
            observed_usage: Some(observed_usage.clone()),
            severity: historical_context_budget_severity(usage_ratio),
        })
    }

    /// Resolves the model capability snapshot associated with one restored session runtime state.
    /// 解析一份恢复后的会话运行状态所关联的模型能力快照。
    fn resolve_runtime_state_model_capabilities(
        &self,
        runtime_state_snapshot: &SessionRuntimeStateEntry,
    ) -> Option<crate::provider_capabilities::ResolvedModelCapabilities> {
        let provider_id = runtime_state_snapshot.active_provider_id.as_ref()?;
        let model_id = runtime_state_snapshot.active_model_id.as_ref()?;
        let definition = provider_config::supported_provider_by_id(provider_id)?;
        let model = provider_config::effective_model_choices(provider_id, &self.provider_configuration)
            .into_iter()
            .find(|model| model.id == *model_id);
        Some(resolve_effective_model_capabilities(
            definition,
            model.as_ref(),
            &self.effective_ai_tool_configuration_for_runtime_options(
                &self.current_pending_prompt_runtime_options(),
            ),
        ))
    }

    /// Synchronizes the visible workbench context bar from the latest session-scoped budget snapshot after compaction or session switching.
    /// 在压缩或会话切换后，根据最新会话级预算快照同步可见工作台上下文条。
    pub(super) fn sync_context_usage_from_session_budget(&mut self, session_id: SessionId) {
        let selected_session_id = self.selected_session().map(|session| session.id);
        if selected_session_id != Some(session_id) {
            return;
        }

        let runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let messages = self.session_context_history(session_id);
        let context_budget = self
            .resolve_session_context_budget_snapshot(
                session_id,
                &runtime_state_snapshot,
                &messages,
                false,
            )
            .or_else(|| {
                self.historical_context_budget_snapshot_from_runtime_state(&runtime_state_snapshot)
            });

        if let Some(context_budget) = context_budget {
            self.context_usage = context_budget.usage_percent();
            self.context_usage_detail = context_budget.summary_note();
            self.visible_context_budget_snapshot = Some(context_budget);
        } else {
            self.context_usage = 0.0;
            self.context_usage_detail = String::from("等待下一次真实请求预算");
            self.visible_context_budget_snapshot = None;
        }
    }

    /// Finalizes one streamed execution report, updates visible session state, and appends hidden derived nodes.
    /// 完成一份流式执行报告，更新可见会话状态并追加隐藏派生节点。
    pub(super) fn apply_execution_run_report(
        &mut self,
        report: ExecutionRunReport,
    ) -> Task<Message> {
        let ExecutionRunReport {
            session_id,
            preparation,
            assistant_output,
            thinking_output,
            conversation_state,
            provider_request_count,
            last_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            error,
            controller_events,
        } = report;
        let is_success = error.is_none();
        let mut perf_trace = TranscriptPerfTrace::begin(
            "apply_execution_run_report",
            format!(
                "session_id={session_id} success={} provider_requests={}",
                is_success, provider_request_count
            ),
        );
        let mut phase_started_at = Instant::now();
        self.last_mcp_discovery_rows = mcp_discovery_rows;
        self.last_mcp_discovery_errors = mcp_discovery_errors;

        if let Some(preparation) = preparation {
            self.sync_context_usage_from_preparation(Some(&preparation));
            perf_trace.checkpoint("apply_preparation.context_usage", phase_started_at);
            phase_started_at = Instant::now();
            self.replace_session_last_api_request_from_preparation(session_id, &preparation);
            self.refresh_prompt_composer_inspector_buffer();
            perf_trace.checkpoint("apply_preparation.request_preview", phase_started_at);
            phase_started_at = Instant::now();
            self.update_session_runtime_identity_from_preparation(session_id, &preparation);
            perf_trace.checkpoint("apply_preparation.runtime_identity", phase_started_at);
            phase_started_at = Instant::now();

            for warning in &preparation.warnings {
                self.push_activity(ActivityLevel::Warning, "执行准备警告", warning.clone());
            }
            perf_trace.checkpoint("apply_preparation.warning_activity", phase_started_at);
            phase_started_at = Instant::now();
            self.last_execution_preparation = Some(preparation);
            perf_trace.checkpoint("apply_preparation.store_preparation", phase_started_at);
            phase_started_at = Instant::now();
        }
        perf_trace.checkpoint("apply_preparation", phase_started_at);
        phase_started_at = Instant::now();

        if error.is_some() {
            if let Some(controller_error) = controller_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, VulcanCoreEventKind::Error))
            {
                self.push_activity(
                    ActivityLevel::Warning,
                    "控制器轨迹错误",
                    format!("{}：{}", controller_error.title, controller_error.body),
                );
            }
        }

        for warning in &warnings {
            self.push_activity(ActivityLevel::Warning, "执行运行警告", warning.clone());
        }
        perf_trace.checkpoint("apply_runtime_warnings", phase_started_at);
        phase_started_at = Instant::now();

        if let Some(next_conversation_state) = conversation_state {
            self.ensure_session_conversation_state_loaded(session_id);
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let preserved_records = runtime_state.conversation_state.records.clone();
            runtime_state.conversation_state = next_conversation_state;
            runtime_state.conversation_state.records = preserved_records;
            let latest_successful_provider_usage =
                last_successful_provider_usage.clone().or_else(|| {
                    if is_success {
                        crate::provider_usage::latest_provider_usage_from_conversation_state(
                            &runtime_state.conversation_state,
                        )
                    } else {
                        None
                    }
                });
            if let Some(successful_usage) = latest_successful_provider_usage {
                runtime_state.last_provider_usage = Some(successful_usage.clone());
                runtime_state.last_successful_provider_usage = Some(successful_usage);
            }
            if provider_request_count > 0 {
                runtime_state.last_safe_provider_request_count = Some(provider_request_count);
            }
            runtime_state.updated_at = current_unix_millis_text();
            self.sync_session_messages_from_conversation_state(session_id);
        }
        perf_trace.checkpoint("sync_conversation_state", phase_started_at);
        phase_started_at = Instant::now();

        let mut summary_assistant_output: Option<String> = None;
        if is_success {
            let thinking_output = thinking_output
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let assistant_output = assistant_output.unwrap_or_else(|| {
                String::from("The model completed the run but did not return visible text output.")
            });
            summary_assistant_output = Some(assistant_output.clone());

            if thinking_output.is_some()
                && !self
                    .streaming_thinking_message_ids
                    .contains_key(&session_id)
            {
                self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::ThinkingOutput,
                    "思考",
                    thinking_output.unwrap_or_default(),
                    true,
                );
            }

            if !self.streaming_output_message_ids.contains_key(&session_id) {
                self.push_visible_session_record_message(
                    session_id,
                    SessionMessageKind::Assistant,
                    SessionMessageRecordKind::AssistantOutput,
                    "输出",
                    assistant_output,
                    false,
                );
            }

            if let Some(session) = self
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Completed;
                session.focus = String::from("最近一次真实执行已完成，可继续追问或调整参数。");
                session.last_updated_label = String::from("刚刚");
            }

            self.set_session_run_phase(session_id, SessionRunPhase::Ready);
            self.status_message = String::from("真实执行已完成，流式结果已写入工作台输出区。");
            self.push_activity(
                ActivityLevel::Success,
                "真实执行已完成",
                format!("会话 #{session_id} 已完成真实模型执行并产出分离后的思考与输出。"),
            );
        } else {
            let error =
                error.unwrap_or_else(|| String::from("真实执行失败，但未返回具体错误信息。"));

            if let Some(session) = self
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Blocked;
                session.last_updated_label = String::from("刚刚");
            }

            self.push_session_message(
                session_id,
                SessionMessageKind::Error,
                "执行失败",
                error.clone(),
            );
            let terminated_message_id = self.append_session_turn_terminated(
                session_id,
                format!("The run terminated without a summary. Error detail: {error}"),
            );
            let _ = self.append_session_turn_stats(
                session_id,
                &terminated_message_id,
                SessionTurnStatsStatus::Failed,
                Some(provider_request_count),
            );
            self.set_session_run_phase(session_id, SessionRunPhase::Failed);
            self.status_message = error.clone();
            self.push_activity(ActivityLevel::Warning, "真实执行失败", error);
        }
        perf_trace.checkpoint("update_visible_outputs", phase_started_at);
        phase_started_at = Instant::now();

        self.refresh_session_queued_steps(session_id);
        if is_success {
            let summary_message_id = self.append_session_summary_output(
                session_id,
                summary_assistant_output.as_deref().unwrap_or(
                    "The model completed the run but did not return visible text output.",
                ),
            );
            let _ = self.append_session_turn_stats(
                session_id,
                &summary_message_id,
                SessionTurnStatsStatus::Completed,
                Some(provider_request_count),
            );
        }
        perf_trace.checkpoint("append_hidden_summary_and_stats", phase_started_at);
        phase_started_at = Instant::now();
        let compaction_task = self.maybe_compact_session_history(session_id);
        self.clear_streaming_message_ids(session_id);
        self.session_tool_call_message_ids.remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
        self.refresh_subagent_navigation_round_visibility(session_id);
        perf_trace.checkpoint("finalize_followup_state", phase_started_at);
        phase_started_at = Instant::now();

        let persist_task = self.persist_workspace_snapshot_task();
        perf_trace.checkpoint("build_persist_task", phase_started_at);
        perf_trace.finish();

        Task::batch([
            compaction_task,
            persist_task,
            self.maybe_follow_selected_session_transcript_tail(session_id),
            self.maybe_start_next_pending_prompt(session_id, false),
        ])
    }

    /// Applies one safe in-flight conversation checkpoint without closing the current run.
    /// 应用一份运行中的安全会话检查点，但不结束当前执行。
    pub(super) fn apply_execution_conversation_checkpoint(
        &mut self,
        session_id: SessionId,
        next_conversation_state: SessionConversationState,
        last_successful_provider_usage: Option<ProviderUsageSnapshot>,
        provider_request_count: usize,
    ) -> Task<Message> {
        self.ensure_session_conversation_state_loaded(session_id);
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        let preserved_records = runtime_state.conversation_state.records.clone();
        runtime_state.conversation_state = next_conversation_state;
        runtime_state.conversation_state.records = preserved_records;
        let latest_successful_provider_usage = last_successful_provider_usage.or_else(|| {
            crate::provider_usage::latest_provider_usage_from_conversation_state(
                &runtime_state.conversation_state,
            )
        });
        if let Some(successful_usage) = latest_successful_provider_usage {
            runtime_state.last_provider_usage = Some(successful_usage.clone());
            runtime_state.last_successful_provider_usage = Some(successful_usage);
        }
        if provider_request_count > 0 {
            runtime_state.last_safe_provider_request_count = Some(provider_request_count);
        }
        runtime_state.updated_at = current_unix_millis_text();
        self.sync_session_messages_from_conversation_state(session_id);
        self.sync_context_usage_from_session_budget(session_id);
        Task::batch([
            self.persist_workspace_snapshot_task(),
            self.maybe_follow_selected_session_transcript_tail(session_id),
        ])
    }

    /// Creates one hidden compaction summary when the visible history grows beyond the current threshold.
    /// 当可见历史增长到当前阈值以上时创建一条隐藏压缩摘要。
    pub(super) fn maybe_compact_session_history(&mut self, session_id: SessionId) -> Task<Message> {
        let mut messages = self.session_context_history(session_id);
        if messages.is_empty() {
            return Task::none();
        }
        let mut runtime_state_snapshot = self
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let mut full_session_messages =
            conversation_state_to_session_messages(&runtime_state_snapshot.conversation_state);
        let Some(context_budget) = self.resolve_session_context_budget_snapshot(
            session_id,
            &runtime_state_snapshot,
            &messages,
            false,
        ) else {
            return Task::none();
        };
        if !context_budget.should_compact() {
            self.sync_context_usage_from_session_budget(session_id);
            return Task::none();
        }
        let Some(mut context_budget) = self.resolve_session_context_budget_snapshot(
            session_id,
            &runtime_state_snapshot,
            &messages,
            true,
        ) else {
            self.sync_context_usage_from_session_budget(session_id);
            return Task::none();
        };
        if !context_budget.baseline_matches_active_runtime {
            self.recover_session_context_anchor_from_baseline_mismatch(session_id, &context_budget);
            messages = self.session_context_history(session_id);
            if messages.is_empty() {
                self.sync_context_usage_from_session_budget(session_id);
                return Task::none();
            }
            runtime_state_snapshot = self
                .session_runtime_state
                .get(&session_id)
                .cloned()
                .unwrap_or_default();
            full_session_messages =
                conversation_state_to_session_messages(&runtime_state_snapshot.conversation_state);
            let Some(recomputed_budget) = self.resolve_session_context_budget_snapshot(
                session_id,
                &runtime_state_snapshot,
                &messages,
                false,
            ) else {
                self.sync_context_usage_from_session_budget(session_id);
                return Task::none();
            };
            context_budget = recomputed_budget;
        }
        let Some(plan) = plan_compaction(CompactionSkillInput {
            session_label: &session_id.to_string(),
            messages: &messages,
            all_messages: &full_session_messages,
            context_start_message_id: runtime_state_snapshot.context_start_message_id.as_deref(),
            policy: ContextCompactionPolicy::default(),
        }) else {
            self.sync_context_usage_from_session_budget(session_id);
            return Task::none();
        };
        self.start_session_prompt_compaction(
            session_id,
            SessionPromptCompactionTrigger::BudgetAuto,
            plan,
            context_budget.summary_note(),
        )
    }

    /// Resets one session's active compaction anchor and stored context fingerprints after the runtime shape no longer matches the previous baseline.
    /// 当运行时形态不再匹配旧基线后，重置单个会话的活动压缩锚点与已存储上下文指纹。
    pub(super) fn recover_session_context_anchor_from_baseline_mismatch(
        &mut self,
        session_id: SessionId,
        context_budget: &ContextBudgetSnapshot,
    ) {
        let runtime_state = self.ensure_session_runtime_state_mut(session_id);
        let had_previous_anchor = runtime_state.last_compaction_id.is_some()
            || runtime_state.context_start_message_id.is_some()
            || runtime_state.anchor_context_budget_fingerprint.is_some()
            || runtime_state.anchor_context_assembly_fingerprint.is_some();
        runtime_state.last_compaction_id = None;
        runtime_state.context_start_message_id = None;
        runtime_state.anchor_context_budget_fingerprint = None;
        runtime_state.anchor_context_assembly_fingerprint = None;
        runtime_state.updated_at = current_unix_millis_text();

        if had_previous_anchor {
            let mismatch_reason = context_budget
                .baseline_mismatch_reason
                .as_deref()
                .unwrap_or("基线结构已变化");
            let detail = format!(
                "会话 #{session_id} 的上下文基线已失配（{mismatch_reason}），已清除旧压缩锚点并回退到完整上下文窗口。"
            );
            self.push_activity(ActivityLevel::Info, "上下文基线恢复", detail.clone());
            if self.selected_session().map(|session| session.id) == Some(session_id) {
                self.status_message = detail;
            }
        }
    }

    /// Builds the current model-facing context history for one session from the start or from the latest compaction anchor.
    /// 从起点或最近一次压缩锚点开始为单个会话构建当前面向模型的上下文历史。
    pub(super) fn session_context_history(
        &self,
        session_id: SessionId,
    ) -> Vec<SessionMessageEntry> {
        self.session_runtime_state(session_id)
            .map(|runtime_state| {
                conversation_state_to_context_records(
                    &runtime_state.conversation_state,
                    runtime_state.context_start_message_id.as_deref(),
                )
            })
            .unwrap_or_default()
    }
}

/// Resolves one historical usage ratio into the same broad severity buckets used by live context budgets.
/// 将一条历史占用比例解析为与实时上下文预算一致的粗粒度严重级别。
fn historical_context_budget_severity(usage_ratio: f32) -> ContextBudgetSeverity {
    if usage_ratio >= vulcan_context::CONTEXT_BUDGET_OVERFLOW_RATIO {
        ContextBudgetSeverity::Overflow
    } else if usage_ratio >= vulcan_context::CONTEXT_BUDGET_COMPACTION_RATIO {
        ContextBudgetSeverity::CompactSoon
    } else if usage_ratio >= vulcan_context::CONTEXT_BUDGET_WARNING_RATIO {
        ContextBudgetSeverity::Warning
    } else {
        ContextBudgetSeverity::Healthy
    }
}

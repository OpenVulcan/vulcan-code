use super::*;

/// Session-level conversation hook dispatcher and turn assembler.
/// 会话级会话 hook 调度器与回合组装器。
#[derive(Clone, Default)]
pub struct ConversationHookRuntime {
    /// Shared hook dispatcher used by every conversation hook point.
    /// 所有会话 hook 点共用的 hook 调度器。
    dispatcher: HookDispatcher,
    /// Active logical turns keyed by session id.
    /// 按会话标识索引的活动逻辑回合。
    pub(super) active_turns: BTreeMap<SessionId, ActiveConversationTurn>,
    /// Monotonic sequence used to generate local turn ids.
    /// 用于生成本地回合标识的单调序列。
    next_turn_sequence: u64,
}

impl ConversationHookRuntime {
    /// Builds an empty conversation hook runtime.
    /// 构建一个空的会话 hook 运行时。
    pub fn new() -> Self {
        let mut runtime = Self::default();
        runtime.register_builtin_points();
        runtime
    }

    /// Returns the mutable shared hook dispatcher for registration.
    /// 返回用于注册的可变共享 hook 调度器。
    pub fn dispatcher_mut(&mut self) -> &mut HookDispatcher {
        &mut self.dispatcher
    }

    /// Returns deterministic snapshots for handlers registered on the precheck hook point.
    /// 返回注册在 precheck hook 点位上的确定性处理器快照。
    pub fn precheck_hook_snapshots(&self) -> Vec<HookRegistrationSnapshot> {
        self.dispatcher
            .registry()
            .registration_snapshots(Some(HOOK_CONVERSATION_TURN_PRECHECK))
    }

    /// Returns whether a logical user turn is currently open for the session.
    /// 返回该会话当前是否存在打开中的逻辑用户回合。
    pub fn has_active_turn(&self, session_id: SessionId) -> bool {
        self.active_turns.contains_key(&session_id)
    }

    /// Registers every built-in conversation hook point descriptor.
    /// 注册所有内置会话 hook 点位描述。
    fn register_builtin_points(&mut self) {
        let descriptors = host_hook_point_descriptors();
        self.dispatcher
            .register_points(descriptors)
            .expect("built-in conversation hook points should be valid");
    }

    /// Opens or updates the logical turn for one user prompt and fires precheck/user hooks.
    /// 为一条用户提示打开或更新逻辑回合，并触发 precheck/user hook。
    pub fn prepare_user_prompt_precheck(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        system_prompt_hook_phase: SystemPromptHookPhase,
        prompt: &str,
        conversation_state: &SessionConversationState,
    ) -> ConversationPrecheckRequest {
        let event_kind = pending_prompt_source_event_kind(source);
        let was_recorded_while_queued = self
            .active_turns
            .get(&session_id)
            .is_some_and(|turn| turn.recorded_prompt_entry_ids.contains(prompt_entry_id));
        let is_new_turn = !self.active_turns.contains_key(&session_id);
        let turn_id = if is_new_turn {
            self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
            let turn_id = format!("session-{session_id}-turn-{}", self.next_turn_sequence);
            self.active_turns.insert(
                session_id,
                ActiveConversationTurn::new(turn_id.clone(), prompt.to_string()),
            );
            turn_id
        } else if was_recorded_while_queued {
            self.active_turns
                .get(&session_id)
                .map(|turn| turn.turn_id.clone())
                .unwrap_or_default()
        } else {
            let turn = self
                .active_turns
                .get_mut(&session_id)
                .expect("active turn should exist after contains_key");
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                None,
            ));
            turn.turn_id.clone()
        };

        let payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "workspace_path": workspace_path,
            "turn_id": turn_id,
            "event_kind": event_kind,
            "is_new_turn": is_new_turn,
            "system_prompt_hook_phase": system_prompt_hook_phase.as_str(),
            "was_recorded_while_queued": was_recorded_while_queued,
            "user_content": prompt,
            "visible_history_count": visible_history_count(conversation_state),
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
        });
        if !was_recorded_while_queued {
            self.dispatch_json(
                HOOK_CONVERSATION_USER_EVENT,
                session_id,
                payload.clone(),
                HookScope::Session,
            );
        }
        ConversationPrecheckRequest::new(self.dispatcher.clone(), session_id, payload, turn_id)
    }

    /// Records a queued user prompt as timeline-only input without running precheck.
    /// 将已排队用户提示记录为仅 timeline 输入，不运行 precheck。
    pub fn record_queued_user_prompt(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        prompt: &str,
    ) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        let event_kind = pending_prompt_source_event_kind(source);
        if let Some(turn) = self.active_turns.get_mut(&session_id)
            && turn
                .recorded_prompt_entry_ids
                .insert(prompt_entry_id.to_string())
        {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                event_kind,
                prompt.trim().to_string(),
                Some(prompt_entry_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_USER_EVENT,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "workspace_path": workspace_path,
                "turn_id": turn_id,
                "event_kind": event_kind,
                "is_new_turn": false,
                "queued": true,
                "prompt_entry_id": prompt_entry_id,
                "user_content": prompt,
            }),
            HookScope::Session,
        );
    }

    /// Records an ask/question prompt opened by the runtime.
    /// 记录运行时打开的 ask/question 提示。
    pub fn record_question_asked(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
    ) {
        let content = questions
            .iter()
            .map(question_prompt_summary)
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "assistant",
                "ask",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ASKED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "questions": question_prompt_payloads(questions),
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user answers submitted to the runtime ask/question prompt.
    /// 记录用户提交给运行时 ask/question 提示的回答。
    pub fn record_question_answered(
        &mut self,
        session_id: SessionId,
        request_id: &str,
        questions: &[QuestionPrompt],
        answers: &[Vec<String>],
    ) {
        let content = answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let title = questions
                    .get(index)
                    .map(|question| question.header.as_str())
                    .unwrap_or("question");
                format!("{title}: {}", answer.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_answer",
                content.clone(),
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_ANSWERED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
                "answers": answers,
                "content": content,
            }),
            HookScope::Session,
        );
    }

    /// Records user rejection of the runtime ask/question prompt.
    /// 记录用户拒绝运行时 ask/question 提示。
    pub fn record_question_rejected(&mut self, session_id: SessionId, request_id: &str) {
        let turn_id = self.ensure_turn_for_runtime_event(session_id);
        if let Some(turn) = self.active_turns.get_mut(&session_id) {
            turn.timeline.push(ConversationTimelineItem::new(
                "user",
                "ask_rejected",
                "The user rejected the runtime question.",
                Some(request_id.to_string()),
            ));
        }
        self.dispatch_json(
            HOOK_CONVERSATION_QUESTION_REJECTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": turn_id,
                "request_id": request_id,
            }),
            HookScope::Session,
        );
    }

    /// Records a tool runtime event when it belongs to a conversation turn.
    /// 当工具运行时事件属于会话回合时记录该事件。
    pub fn record_tool_runtime_event(&mut self, session_id: SessionId, event: &RuntimeEvent) {
        let Some(turn_id) = self
            .active_turns
            .get(&session_id)
            .map(|turn| turn.turn_id.clone())
        else {
            return;
        };
        self.record_tool_runtime_event_for_execution(session_id, &turn_id, event);
    }

    /// Dispatches a tool hook for an explicit execution without creating a conversation turn.
    /// 为显式执行派发工具 hook，且不创建会话回合。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the child or standalone execution session that emitted the event.
    /// `session_id` 标识发出事件的子代理或独立执行会话。
    /// `execution_id` identifies the exact execution generation used as the hook turn correlation id.
    /// `execution_id` 标识作为 hook 回合关联标识的精确执行代次。
    /// `event` is the runtime event; non-tool boundaries are ignored.
    /// `event` 是运行时事件；非工具边界事件会被忽略。
    ///
    /// # Returns
    /// 返回值
    /// This method returns no value and never mutates active conversation turns.
    /// 该方法无返回值，且绝不修改活跃会话回合。
    pub fn record_tool_runtime_event_for_execution(
        &self,
        session_id: SessionId,
        execution_id: &str,
        event: &RuntimeEvent,
    ) {
        let (hook_point, timeline_kind) = match event.kind {
            RuntimeEventKind::ToolStarted => (HOOK_CONVERSATION_TOOL_STARTED, "tool_started"),
            RuntimeEventKind::ToolFinished => (HOOK_CONVERSATION_TOOL_FINISHED, "tool_finished"),
            _ => return,
        };
        let source_id = event
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // Tool events still dispatch hook notifications, but they do not belong in the user-turn timeline.
        // 工具事件仍会派发 hook 通知，但不进入用户回合 timeline。
        self.dispatch_json(
            hook_point,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "turn_id": execution_id,
                "kind": timeline_kind,
                "title": event.title,
                "detail": event.detail,
                "source_id": source_id,
                "metadata": event.metadata,
            }),
            HookScope::Session,
        );
    }

    /// Completes one logical turn and fires the completed hook payload.
    /// 完成一个逻辑回合并触发 completed hook payload。
    pub fn complete_turn(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
    ) -> (String, Vec<VmmWritebackHookEffect>) {
        self.complete_turn_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        )
    }

    /// Dispatches a completed-turn hook payload with an explicit logical completion state.
    /// 使用显式逻辑完成状态调度完成回合 hook 载荷。
    pub fn complete_turn_with_status(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
        completion_status: ConversationTurnCompletionStatus,
        pending_subagent_count: usize,
    ) -> (String, Vec<VmmWritebackHookEffect>) {
        let (completed_turn_id, request) = self.prepare_turn_completion_dispatch_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            completion_status,
            pending_subagent_count,
        );
        let report = request.run_blocking();
        (
            completed_turn_id,
            report
                .as_ref()
                .map(vmm_writeback_effects_from_report)
                .unwrap_or_default(),
        )
    }

    /// Completes one logical turn locally and returns a dispatch request that can run off the critical path.
    /// 在本地闭合一个逻辑回合，并返回可脱离关键路径执行的调度请求。
    pub fn prepare_turn_completion_dispatch(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
    ) -> (String, ConversationCompletionRequest) {
        self.prepare_turn_completion_dispatch_with_status(
            session_id,
            project_id,
            assistant_content,
            provider_request_count,
            conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        )
    }

    /// Prepares one completed-turn hook payload while preserving unfinished logical turns when required.
    /// 准备一条完成回合 hook 载荷，并在需要时保留尚未结束的逻辑回合。
    pub fn prepare_turn_completion_dispatch_with_status(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        assistant_content: &str,
        provider_request_count: usize,
        conversation_state: &SessionConversationState,
        completion_status: ConversationTurnCompletionStatus,
        pending_subagent_count: usize,
    ) -> (String, ConversationCompletionRequest) {
        let turn = if completion_status.is_final() {
            self.active_turns
                .remove(&session_id)
                .unwrap_or_else(|| self.reconstruct_turn_from_state(session_id, conversation_state))
        } else if let Some(turn) = self.active_turns.get(&session_id) {
            turn.clone()
        } else {
            let turn = self.reconstruct_turn_from_state(session_id, conversation_state);
            self.active_turns.insert(session_id, turn.clone());
            turn
        };
        let completed_turn_id = turn.turn_id.clone();
        let turn_complete = completion_status.is_final();
        let payload = json!({
            "session_id": session_id.to_string(),
            "project_id": project_id,
            "turn_id": turn.turn_id,
            "user_content": turn.user_content,
            "timeline": turn.timeline,
            "assistant_content": assistant_content,
            "provider_request_count": provider_request_count,
            "completion_status": completion_status.as_str(),
            "turn_complete": turn_complete,
            "pending_subagent_count": pending_subagent_count,
            "message_count": conversation_state.messages.len(),
            "record_count": conversation_state.records.len(),
            "turn_messages": &conversation_state.messages,
            "turn_records": &conversation_state.records,
            "conversation_state": conversation_state,
        });
        (
            completed_turn_id.clone(),
            ConversationCompletionRequest::new(
                self.dispatcher.clone(),
                session_id,
                payload,
                completed_turn_id,
            ),
        )
    }

    /// Dispatches one prepared completed-turn payload synchronously for legacy callers.
    /// 为旧调用方同步调度一条已准备的完成回合 payload。
    pub(super) fn dispatch_completion_blocking(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
    ) -> Option<HookDispatchReport> {
        Self::dispatch_json_blocking_with_dispatcher(
            dispatcher,
            session_id,
            payload,
            HOOK_CONVERSATION_TURN_COMPLETED,
            COMPLETION_DISPATCH_TIMEOUT_MS,
        )
    }

    /// Terminates one logical turn and fires the terminated hook payload.
    /// 终止一个逻辑回合并触发 terminated hook payload。
    pub fn terminate_turn(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        reason: &str,
        provider_request_count: usize,
    ) {
        let turn = self.active_turns.remove(&session_id);
        self.dispatch_json(
            HOOK_CONVERSATION_TURN_TERMINATED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "turn_id": turn.as_ref().map(|turn| turn.turn_id.as_str()),
                "user_content": turn.as_ref().map(|turn| turn.user_content.as_str()),
                "timeline": turn.as_ref().map(|turn| &turn.timeline),
                "reason": reason,
                "provider_request_count": provider_request_count,
            }),
            HookScope::Session,
        );
    }

    /// Records a completed compaction boundary.
    /// 记录一次已完成的压缩边界。
    pub fn record_compacted(
        &mut self,
        session_id: SessionId,
        project_id: Option<String>,
        trigger: &str,
        fallback_used: bool,
        provider_request_count: usize,
        summary: &str,
    ) {
        self.dispatch_json(
            HOOK_CONVERSATION_COMPACTED,
            session_id,
            json!({
                "session_id": session_id.to_string(),
                "project_id": project_id,
                "trigger": trigger,
                "fallback_used": fallback_used,
                "provider_request_count": provider_request_count,
                "summary": summary,
            }),
            HookScope::Session,
        );
    }

    /// Reconstructs a minimal turn from canonical state when active state is missing.
    /// 当活动状态缺失时从规范状态重建一个最小回合。
    pub(super) fn reconstruct_turn_from_state(
        &mut self,
        session_id: SessionId,
        conversation_state: &SessionConversationState,
    ) -> ActiveConversationTurn {
        self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
        let user_content = conversation_state
            .messages
            .iter()
            .rev()
            .find_map(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        ActiveConversationTurn::new(
            format!("session-{session_id}-turn-{}", self.next_turn_sequence),
            user_content,
        )
    }

    /// Ensures runtime-only events still have a turn container.
    /// 确保仅运行时事件仍然拥有回合容器。
    fn ensure_turn_for_runtime_event(&mut self, session_id: SessionId) -> String {
        if let Some(turn) = self.active_turns.get(&session_id) {
            return turn.turn_id.clone();
        }
        self.next_turn_sequence = self.next_turn_sequence.saturating_add(1);
        let turn_id = format!("session-{session_id}-turn-{}", self.next_turn_sequence);
        self.active_turns.insert(
            session_id,
            ActiveConversationTurn::new(turn_id.clone(), String::new()),
        );
        turn_id
    }

    /// Dispatches one JSON hook payload without blocking the UI thread.
    /// 在不阻塞 UI 线程的情况下调度一条 JSON hook payload。
    fn dispatch_json(
        &self,
        point: &'static str,
        session_id: SessionId,
        payload: Value,
        scope: HookScope,
    ) {
        let dispatcher = self.dispatcher.clone();
        let context = HookContext {
            run_id: format!("{point}:{session_id}"),
            point: point.to_string(),
            scope,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(context, HookPayload { value: payload });
        spawn_hook_dispatch(dispatcher, request);
    }

    /// Dispatches one JSON hook payload synchronously through the provided dispatcher.
    /// 通过给定调度器同步调度一条 JSON hook payload。
    fn dispatch_json_blocking_with_dispatcher(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        point: &'static str,
        timeout_ms: u64,
    ) -> Option<HookDispatchReport> {
        let context = HookContext {
            run_id: format!("{point}:{session_id}"),
            point: point.to_string(),
            scope: HookScope::Session,
            actor: HookActor::new("vulcan-code-runtime"),
            workspace_id: None,
            project_id: None,
            session_id: Some(session_id.to_string()),
            conversation_id: Some(session_id.to_string()),
            user_id: None,
            capabilities: Default::default(),
            metadata: BTreeMap::new(),
        };
        let request = HookDispatchRequest::new(context, HookPayload { value: payload });
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            runtime.block_on(async move {
                tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    dispatcher.dispatch(request),
                )
                .await
                .ok()
                .and_then(Result::ok)
            })
        })
        .join()
        .ok()
        .flatten()
    }
}
